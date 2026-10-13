//! EMS manufacturer backend CLI (reference client): runs one zero-touch flow against a
//! configured HEMS stub + MSB platforms. Reads all inputs from env (no CLI args needed).

use ems_backend::{audit, regel_s_base, shipqr, wellknown, ClientConfig};
use serde_json::json;

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    tracing_subscriber::fmt()
        .with_env_filter(
            tracing_subscriber::EnvFilter::try_from_default_env()
                .unwrap_or_else(|_| "info,rustls=warn,hyper=warn".into()),
        )
        .init();

    let root = env_or("EMS_ROOT_BUNDLE", "test/ca/out/server-roots.pem");
    let client_cert = std::env::var("EMS_CLIENT_CERT").ok();
    let client_key = std::env::var("EMS_CLIENT_KEY").ok();
    let platforms: Vec<String> = env_or("EMS_PLATFORM_URLS", "https://127.0.0.1:18443")
        .split(',')
        .map(|s| s.trim().trim_end_matches('/').to_string())
        .filter(|s| !s.is_empty())
        .collect();
    let wellknown_url = env_or("EMS_WELLKNOWN_URL", "https://127.0.0.1:19443");
    let hems_url = env_or("EMS_HEMS_URL", "http://127.0.0.1:18643");

    // 1. well-known + TOFU client-CA pin (server-TLS only)
    let wk_cfg = ClientConfig {
        root_bundle_path: root.clone(),
        client_cert_path: None,
        client_key_path: None,
    };
    let wk_client = wk_cfg.build_http_client()?;
    let doc = wellknown::fetch(&wk_client, &wellknown_url).await?;
    let pin_path = env_or("EMS_CLIENT_CA_PIN", "/tmp/steuerbox-connect-client-ca.pin");
    // the published pem is base64-encoded; the pin store holds the decoded PEM
    let ca_pem = wellknown::decode_client_ca_pem(&doc.client_ca.pem)?;
    let decision = wellknown::pin_tofu(&pin_path, &ca_pem, false)?;
    anyhow::ensure!(
        decision != wellknown::PinDecision::Mismatch,
        "well-known client CA differs from the qualified TOFU pin — refusing provisioning"
    );
    tracing::info!(manufacturer = %doc.iana_pen, pin = ?decision, "well-known fetched and pinned (TOFU)");
    let _ = &doc;

    // 2. read the synthetic LAN observation reported by the HEMS stub
    let http = reqwest::Client::new();
    let evidence: serde_json::Value = http
        .get(format!("{hems_url}/evidence"))
        .send()
        .await?
        .error_for_status()?
        .json()
        .await?;
    let box_info = evidence
        .get("box")
        .cloned()
        .ok_or_else(|| anyhow::anyhow!("no Steuerbox observed in the LAN"))?;
    let txt_serial = box_info
        .get("serial")
        .and_then(|v| v.as_str())
        .unwrap_or_default();
    let mut hems = evidence.get("hems").cloned().unwrap_or(json!({}));

    // Fleet binding, client-side (ADR-0018): the HEMS SHIP-ID must carry the IANA PEN
    // published in our own well-known document — refuse to send foreign-fleet data
    // (defense in depth; the definitive check is server-side, 422 fleet_mismatch).
    let hems_ship_id = hems.get("ship_id").and_then(|v| v.as_str()).unwrap_or("");
    anyhow::ensure!(
        ems_backend::matches_fleet(&doc.iana_pen, hems_ship_id),
        "HEMS ship_id does not carry the well-known iana_pen — refusing to provision foreign-fleet data"
    );

    // 3. hems_eebus_local_qr: RSA-decrypt (EMS QR keypair) WITH audit logging — metadata
    //    only (HEMS serial, MSB, timestamp, outcome), never the plaintext.
    let msb_id = doc.iana_pen.clone();
    let audit_log = audit::AuditLog::open(&env_or("EMS_AUDIT_LOG", "audit.log"))?;
    let mut parsed_qr: Option<shipqr::ShipQr> = None;
    if let Some(qr) = evidence.get("local_qr").and_then(|v| v.as_str()) {
        let outcome = match decrypt_and_check(qr, &hems) {
            Ok(qr) => {
                tracing::info!(target: "AUDIT", hems_serial = %hems["serial"], msb_id, "QR decrypted — local identity cross-check passed");
                // FPH256 from the QR is authoritative for the pairing dataset
                hems["fph"] = json!(qr.fph256.to_lowercase());
                parsed_qr = Some(qr);
                audit::DecryptOutcome::Success
            }
            Err(e) => {
                tracing::error!(target: "AUDIT", error = %e, "QR decryption failed");
                audit::DecryptOutcome::Failure
            }
        };
        audit_log.record(&audit::AuditEvent {
            ts_ms: std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .map(|d| d.as_millis())
                .unwrap_or(0),
            action: "qr_decrypt",
            hems_serial: hems
                .get("serial")
                .and_then(|v| v.as_str())
                .unwrap_or_default()
                .to_string(),
            msb_id: msb_id.clone(),
            outcome,
        })?;
        if outcome == audit::DecryptOutcome::Failure {
            anyhow::bail!("QR decryption failed — refusing to provision without the local HEMS identity anchor");
        }
    }

    // Regel-S: derivation from the observed TXT record → discovery query
    let query = regel_s_base(txt_serial);
    tracing::info!(
        query,
        txt_serial,
        "derived discovery query from LAN observation"
    );

    // 3. zero-touch: fan-out discovery → canonical serial → provisioning
    let cfg = match (&client_cert, &client_key) {
        (Some(c), Some(k)) => ClientConfig::mtls(&root, c, k),
        _ => anyhow::bail!("EMS_CLIENT_CERT/EMS_CLIENT_KEY required for mTLS"),
    };
    let client = cfg.build_http_client()?;

    // qr_info: canonical SHIP-Info-Datensatz built from the decrypted QR content
    // (ADR-0017). format_canonical carries the FULL QR content INCLUDING the SPSEC
    // security code — the payload (mTLS) is the secret's transport to the MSB; it
    // is never logged (metadata only) and reaches the box via the MSB admin
    // channel. Fallback: the hems-reported SHIP string when no QR was observable.
    let qr_info = parsed_qr
        .map(|qr| qr.format_canonical())
        .unwrap_or_else(|| {
            hems.get("qr_info")
                .and_then(|v| v.as_str())
                .unwrap_or("")
                .to_string()
        });

    let pairing_body = json!({
        "pairing_dataset": {
            "ship_id": hems.get("ship_id").and_then(|v| v.as_str()).unwrap_or(""),
            "ski": hems.get("ski").and_then(|v| v.as_str()).unwrap_or(""),
            "qr_info": qr_info,
        },
        "box_evidence": {
            "ship_id": box_info.get("ship_id"),
            "ski": box_info.get("ski"),
        }
    });

    let (canonical, outcome) =
        ems_backend::zero_touch(&client, &platforms, query, &pairing_body).await?;
    tracing::info!(canonical, ?outcome, "zero-touch flow complete");
    println!("RESULT canonical={canonical} outcome={outcome:?}");
    Ok(())
}

use anyhow::Context;

fn env_or(name: &str, default: &str) -> String {
    std::env::var(name).unwrap_or_else(|_| default.to_string())
}

/// Decrypt the QR content (LOCAL HEMS data only, real SHIP format) and cross-check the
/// decrypted SKI/ID/FPH against the HEMS-reported `hems_eebus_local_*` fields.
/// Returns the parsed [`shipqr::ShipQr`] — FPH256 is authoritative for the pairing dataset.
fn decrypt_and_check(
    qr_cipher_b64: &str,
    hems: &serde_json::Value,
) -> anyhow::Result<shipqr::ShipQr> {
    let key_pem = std::fs::read_to_string(env_or("EMS_QR_KEY", "test/ca/out/ems-qr-key.pem"))?;
    let plaintext = audit::decrypt_qr(&key_pem, qr_cipher_b64)?;
    let content = String::from_utf8(plaintext).context("QR plaintext is not UTF-8")?;
    let qr = shipqr::ShipQr::parse(&content)?;

    // QR anchors the LOCAL HEMS identity — cross-check against the reported fields
    let reported_ski = hems.get("ski").and_then(|v| v.as_str()).unwrap_or("");
    let reported_ship_id = hems.get("ship_id").and_then(|v| v.as_str()).unwrap_or("");
    anyhow::ensure!(
        !qr.ski.is_empty() && qr.ski == reported_ski,
        "decrypted QR SKI does not match reported hems_eebus_local_ski — identity rejected"
    );
    anyhow::ensure!(
        qr.id == reported_ship_id,
        "decrypted QR ID does not match reported hems_eebus_local_ship_id — identity rejected"
    );
    // FPH256 must match the HEMS-reported fph (both are the FNN credential fingerprint)
    let reported_fph = hems.get("fph").and_then(|v| v.as_str()).unwrap_or("");
    anyhow::ensure!(
        reported_fph.is_empty() || reported_fph.eq_ignore_ascii_case(&qr.fph256),
        "QR FPH256 does not match reported hems fph — credential rejected"
    );
    Ok(qr)
}

#[cfg(test)]
mod tests {
    use super::regel_s_base;

    #[test]
    fn regel_s_derivation() {
        assert_eq!(regel_s_base("SNTEST0001-A1"), "SNTEST0001");
        assert_eq!(regel_s_base("SNTEST0001"), "SNTEST0001");
    }
}
