//! HEMS stub (LAN side, reference stub — never a fake of our own components):
//! - synthetic `_ship._tcp` TXT observation input through the normative parser
//! - exposes exactly the fields the EMS backend persists (`hems_eebus_*` model):
//!   stb_connected, local_ski, local_ship_id, local_qr, stb_found, stb_ship_id, stb_ski
//! - RSA-encrypts the QR content with the EMS-backend QR public key (ems-backend decrypts + audits)
//! - SHIP connect to the box: TLS + one `SKI:<local_ski>` line (closed-loop probe)

mod txt;

use std::sync::Arc;

use axum::extract::State;
use axum::http::StatusCode;
use axum::response::IntoResponse;
use axum::routing::{get, post};
use axum::{Json, Router};
use rsa::pkcs8::DecodePublicKey;
use serde_json::json;
use sha2::Digest;

#[derive(Clone)]
struct HemsState {
    hems_serial: String,
    local_ship_id: String,
    local_ski: String,
    brand: String,
    model: String,
    cat: String,
    fph: String,
    qr_pub: rsa::RsaPublicKey,
    observed: Arc<tokio::sync::RwLock<Option<txt::TxtRecord>>>,
}

fn env_or(name: &str, default: &str) -> String {
    std::env::var(name).unwrap_or_else(|_| default.to_string())
}

/// RSA-encrypt the QR content with the EMS-backend QR keypair, base64-encoded (like
/// hems_support_password). Plaintext is the real SHIP pairing format — ONLY local HEMS
/// data (SKI, ID, FPH256, …), never Steuerbox data. Decryption in the backend is audited.
fn encrypt_qr(state: &HemsState) -> anyhow::Result<String> {
    let content = ship_qr_content(state);
    use rsa::Oaep;
    let mut rng = rand::thread_rng();
    let cipher = state
        .qr_pub
        .encrypt(&mut rng, Oaep::new::<sha2::Sha256>(), content.as_bytes())?;
    use base64::Engine;
    Ok(base64::engine::general_purpose::STANDARD.encode(cipher))
}

/// Real-world SHIP QR content shape:
/// `SHIP;SKI:…;ID:…;BRAND:…;TYPE:EnergyManagementSystem;MODEL:…;SERIAL:…;CAT:…;
///  FPH256:…;SPSEC:…;ENDSHIP;`
fn ship_qr_content(state: &HemsState) -> String {
    use rand::RngCore;
    let mut spsec = [0u8; 8];
    rand::thread_rng().fill_bytes(&mut spsec);
    format!(
        "SHIP;SKI:{ski};ID:{id};BRAND:{brand};TYPE:EnergyManagementSystem;MODEL:{model};SERIAL:{serial};CAT:{cat};FPH256:{fph};SPSEC:{spsec};ENDSHIP;",
        ski = state.local_ski,
        id = state.local_ship_id,
        brand = state.brand,
        model = state.model,
        serial = state.hems_serial,
        cat = state.cat,
        fph = state.fph.to_uppercase(),
        spsec = hex::encode(spsec),
    )
}

/// Secret-bearing SHIP-Info-Datensatz for the evidence `hems.qr_info` field
/// (ADR-0017): the FULL SHIP string INCLUDING a fresh SPSEC security code —
/// the provisioning payload carries the secret (mTLS is its transport; the MSB
/// forwards it only via the box admin channel and never logs it). The QR channel
/// (local_qr, RSA-encrypted) is the primary source; this is the no-QR fallback.
fn ship_info_dataset(state: &HemsState) -> String {
    use rand::RngCore;
    let mut spsec = [0u8; 8];
    rand::thread_rng().fill_bytes(&mut spsec);
    format!(
        "SHIP;SKI:{ski};ID:{id};BRAND:{brand};TYPE:EnergyManagementSystem;MODEL:{model};SERIAL:{serial};CAT:{cat};FPH256:{fph};SPSEC:{spsec};ENDSHIP;",
        ski = state.local_ski,
        id = state.local_ship_id,
        brand = state.brand,
        model = state.model,
        serial = state.hems_serial,
        cat = state.cat,
        fph = state.fph.to_uppercase(),
        spsec = hex::encode(spsec),
    )
}

// --- evidence endpoint: the EMS backend's hems_eebus_* field model -------------------

async fn evidence(State(state): State<HemsState>) -> impl IntoResponse {
    let observed = state.observed.read().await.clone();
    let stb_found = observed.is_some();
    let box_json = observed.as_ref().map(|stb| {
        json!({
            "serial": stb.regel_s_serial(),
            "ship_id": stb.box_ship_id(),
            "ski": stb.ski,
            "type": stb.stb_type,
            "model": stb.model,
        })
    });

    let encrypt_qr_result = match encrypt_qr(&state) {
        Ok(qr) => Some(qr),
        Err(e) => {
            tracing::error!(error = %e, "QR encryption failed — hems_eebus_local_qr absent");
            None
        }
    };

    Json(json!({
        "hems": {
            "ship_id": state.local_ship_id,
            "ski": state.local_ski,
            "qr_info": ship_info_dataset(&state),
            "serial": state.hems_serial,
        },
        "stb_found": stb_found,
        "stb_connected": stb_found,
        "box": box_json,
        "local_qr": encrypt_qr_result,
    }))
}

// --- SHIP connect probe (closed loop) ------------------------------------------------

async fn ship_connect(
    State(state): State<HemsState>,
    Json(body): Json<serde_json::Value>,
) -> impl IntoResponse {
    let Some(addr) = body.get("addr").and_then(|v| v.as_str()).map(String::from) else {
        return (
            StatusCode::BAD_REQUEST,
            Json(json!({"error": "addr required"})),
        )
            .into_response();
    };
    // optional ski override for negative probes (e2e: wrong SKI must be rejected)
    let probe_state = match body.get("ski").and_then(|v| v.as_str()) {
        Some(ski) => {
            let mut s = state.clone();
            s.local_ski = ski.to_string();
            s
        }
        None => state,
    };
    match ship_probe(&probe_state, &addr).await {
        Ok(accepted) => (
            StatusCode::OK,
            Json(json!({ "reply": if accepted { "ACCEPTED" } else { "REJECTED" }, "accepted": accepted })),
        )
            .into_response(),
        Err(e) => (
            StatusCode::BAD_GATEWAY,
            Json(json!({ "error": e.to_string(), "accepted": false })),
        )
            .into_response(),
    }
}

#[derive(Debug)]
struct NoVerify;

impl rustls::client::danger::ServerCertVerifier for NoVerify {
    fn verify_server_cert(
        &self,
        _end_entity: &rustls_pki_types::CertificateDer<'_>,
        _intermediates: &[rustls_pki_types::CertificateDer<'_>],
        _server_name: &rustls_pki_types::ServerName<'_>,
        _ocsp: &[u8],
        _now: rustls_pki_types::UnixTime,
    ) -> Result<rustls::client::danger::ServerCertVerified, rustls::Error> {
        // The box simulates SHIP-TLS with a self-signed cert; SKI acceptance is the
        // LAN-side rule, so any server certificate is accepted by this stub.
        Ok(rustls::client::danger::ServerCertVerified::assertion())
    }

    fn verify_tls12_signature(
        &self,
        _message: &[u8],
        _cert: &rustls_pki_types::CertificateDer<'_>,
        _dss: &rustls::DigitallySignedStruct,
    ) -> Result<rustls::client::danger::HandshakeSignatureValid, rustls::Error> {
        Ok(rustls::client::danger::HandshakeSignatureValid::assertion())
    }

    fn verify_tls13_signature(
        &self,
        _message: &[u8],
        _cert: &rustls_pki_types::CertificateDer<'_>,
        _dss: &rustls::DigitallySignedStruct,
    ) -> Result<rustls::client::danger::HandshakeSignatureValid, rustls::Error> {
        Ok(rustls::client::danger::HandshakeSignatureValid::assertion())
    }

    fn supported_verify_schemes(&self) -> Vec<rustls::SignatureScheme> {
        vec![
            rustls::SignatureScheme::RSA_PKCS1_SHA256,
            rustls::SignatureScheme::ECDSA_NISTP256_SHA256,
            rustls::SignatureScheme::ED25519,
        ]
    }
}

/// TLS dial to the box and send one `SKI:<ski>` line. Default: the HEMS's own local_ski;
/// the e2e suite overrides it for negative probes (wrong SKI must be rejected).
async fn ship_probe(state: &HemsState, addr: &str) -> anyhow::Result<bool> {
    let payload = format!("SKI:{}\n", state.local_ski);
    let config = rustls::ClientConfig::builder()
        .dangerous()
        .with_custom_certificate_verifier(Arc::new(NoVerify))
        .with_no_client_auth();
    let config = Arc::new(config);

    let server_name = rustls_pki_types::ServerName::try_from("steuerbox.local".to_string())?;
    let mut tls = tokio_rustls::TlsConnector::from(config)
        .connect(server_name, tokio::net::TcpStream::connect(addr).await?)
        .await?;

    use tokio::io::{AsyncReadExt, AsyncWriteExt};
    tls.write_all(payload.as_bytes()).await?;
    let mut buf = vec![0u8; 128];
    let n = tls.read(&mut buf).await?;
    let reply = String::from_utf8_lossy(&buf[..n]).trim().to_string();
    let accepted = reply == "ACCEPTED";
    tracing::info!(reply, accepted, "SHIP handshake result");
    Ok(accepted)
}

// --- cloud-side observation input ----------------------------------------------------

// (browse_mdns removed: purely cloud-side — the HEMS data has already been received)

/// Cloud-side input: the TXT record the reference HEMS received via mDNS and forwarded
/// with its `hems_eebus_*` report. Injected through the SAME normative parser — never
/// a different code path.
fn load_observed_txt() -> anyhow::Result<txt::TxtRecord> {
    let json_txt = std::env::var("HEMS_TXT_RECORD")?;
    let entries: Vec<String> = serde_json::from_str(&json_txt)?;
    let refs: Vec<&str> = entries.iter().map(|s| s.as_str()).collect();
    let rec = txt::TxtRecord::parse(&refs);
    anyhow::ensure!(rec.id.is_some(), "HEMS TXT record must contain id= (VPID)");
    tracing::info!(
        serial = ?rec.regel_s_serial(),
        vpid = ?rec.box_ship_id(),
        "Steuerbox observation loaded (cloud-side, as reported by the reference HEMS)"
    );
    Ok(rec)
}

fn load_qr_pub() -> anyhow::Result<rsa::RsaPublicKey> {
    let path = env_or("EMS_QR_PUB_PEM", "test/ca/out/ems-qr-pub.pem");
    let pem = std::fs::read_to_string(&path)?;
    Ok(rsa::RsaPublicKey::from_public_key_pem(&pem)?)
}

#[allow(dead_code)]
fn install_crypto_provider() {
    // deterministic crypto provider (workspace pins rustls/ring; reqwest may also pull aws-lc-rs)
    let _ = rustls::crypto::ring::default_provider().install_default();
}

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    install_crypto_provider();
    tracing_subscriber::fmt()
        .with_env_filter(
            tracing_subscriber::EnvFilter::try_from_default_env()
                .unwrap_or_else(|_| "info,rustls=warn,tokio_rustls=warn".into()),
        )
        .init();

    let hems_serial = env_or("HEMS_SERIAL", "SNTEST_HEMS-0001");
    // HEMS ship_id in SHIP-ID shape `i:<IANA-PEN>_u:<VPID>` (ADR-0018): the PEN part
    // must match the iana_pen the well-known server publishes for this manufacturer.
    let local_ship_id = env_or(
        "HEMS_LOCAL_SHIP_ID",
        &format!("i:12345_u:ExampleEMS-HEMS-{hems_serial}"),
    );
    // FPH256 (FNN credential fingerprint): SHA-256 over the local identity (mock semantics)
    let mut hasher = sha2::Sha256::new();
    hasher.update(local_ship_id.as_bytes());
    let fph = hex::encode(hasher.finalize());
    let state = HemsState {
        hems_serial: hems_serial.clone(),
        local_ship_id: local_ship_id.clone(),
        local_ski: env_or("HEMS_LOCAL_SKI", &random_ski()),
        brand: env_or("HEMS_BRAND", "ExampleEMS"),
        model: env_or("HEMS_MODEL", "Reference-HEMS"),
        cat: env_or("HEMS_CAT", "2"),
        fph: env_or("HEMS_FPH", &fph),
        qr_pub: load_qr_pub()?,
        observed: Arc::new(tokio::sync::RwLock::new(load_observed_txt().ok())),
    };

    if state
        .observed
        .try_read()
        .map(|o| o.is_none())
        .unwrap_or(true)
    {
        tracing::warn!("no HEMS_TXT_RECORD provided — hems_eebus_stb_found will be false");
    }

    let bind: std::net::SocketAddr = env_or("HEMS_BIND", "0.0.0.0:18643").parse()?;
    let app = Router::new()
        .route("/evidence", get(evidence))
        .route("/ship-connect", post(ship_connect))
        .with_state(state);
    let listener = tokio::net::TcpListener::bind(bind).await?;
    tracing::info!(%bind, "hems-mock up");
    tokio::select! {
        result = async { axum::serve(listener, app).await } => { result?; },
        _ = steuerbox_protocol::shutdown_signal() => {},
    }
    Ok(())
}

fn random_ski() -> String {
    use rand::RngCore;
    let mut buf = [0u8; 20];
    rand::thread_rng().fill_bytes(&mut buf);
    hex::encode(buf)
}
