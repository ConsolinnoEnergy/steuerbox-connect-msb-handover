//! Well-known document fetch + client-CA pinning (TOFU + TTL + rotation grace).
//!
//! The document (spec/wellknown.schema.json) publishes the manufacturer's dedicated
//! client CA (`scope: steuerbox-connect-api`). The MSB pins the public part; the client
//! keeps its own key material. Pin persistence is a TOFU file: first fetch stores the
//! PEM; later fetches compare and detect rotation.

use anyhow::{Context, Result};
use base64::Engine as _;
use serde::{Deserialize, Serialize};
use time::OffsetDateTime;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct WellKnownDoc {
    /// IANA PEN of the EMS manufacturer, e.g. `12345` (ADR-0018 fleet binding).
    pub iana_pen: String,
    pub client_ca: ClientCa,
    pub contact: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ClientCa {
    pub pem: String,
    pub scope: String, // MUST be "steuerbox-connect-api"
    #[serde(default)]
    pub not_before: Option<String>,
    #[serde(default)]
    pub not_after: Option<String>,
}

pub const WELLKNOWN_PATH: &str = "/.well-known/steuerbox-connect/v1";

/// The published `client_ca.pem` is base64-encoded (wellknown.schema.json,
/// contentEncoding: base64). Decode to the plain PEM for the pin store.
pub fn decode_client_ca_pem(pem_b64: &str) -> Result<String> {
    let bytes = base64::engine::general_purpose::STANDARD
        .decode(pem_b64.trim())
        .context("client_ca.pem is not valid base64")?;
    let pem = String::from_utf8(bytes).context("decoded client_ca.pem is not UTF-8")?;
    anyhow::ensure!(
        pem.contains("-----BEGIN CERTIFICATE-----"),
        "decoded client_ca.pem does not contain a certificate"
    );
    Ok(pem)
}

fn now_unix() -> OffsetDateTime {
    OffsetDateTime::now_utc()
}

/// Validate the CA's declared validity window (rotation grace, ADR-0003). RFC 3339
/// timestamps; `None` bounds are tolerated (SHOULD-level). Fetching outside the window
/// fails — a silently expired pin is a rotation bug.
pub fn validate_ca_window(
    not_before: Option<&str>,
    not_after: Option<&str>,
    now: OffsetDateTime,
) -> Result<()> {
    let parse = |s: &str| {
        OffsetDateTime::parse(s, &time::format_description::well_known::Rfc3339)
            .map_err(|e| anyhow::anyhow!("invalid RFC 3339 timestamp {s:?}: {e}"))
    };
    if let Some(nb) = not_before {
        anyhow::ensure!(
            now >= parse(nb)?,
            "client CA not yet valid (not_before {nb})"
        );
    }
    if let Some(na) = not_after {
        anyhow::ensure!(now <= parse(na)?, "client CA expired (not_after {na})");
    }
    Ok(())
}

/// Fetch the well-known document over server-TLS (no client cert needed).
pub async fn fetch(client: &reqwest::Client, base_url: &str) -> Result<WellKnownDoc> {
    let url = format!("{}{}", base_url.trim_end_matches('/'), WELLKNOWN_PATH);
    let doc: WellKnownDoc = client
        .get(&url)
        .send()
        .await?
        .error_for_status()?
        .json()
        .await?;
    anyhow::ensure!(
        doc.client_ca.scope == "steuerbox-connect-api",
        "client_ca scope must be steuerbox-connect-api"
    );
    // TTL / rotation grace: honor the published window before pinning.
    let _pem = decode_client_ca_pem(&doc.client_ca.pem)?;
    validate_ca_window(
        doc.client_ca.not_before.as_deref(),
        doc.client_ca.not_after.as_deref(),
        now_unix(),
    )
    .context("client CA validity window (TTL/grace)")?;
    Ok(doc)
}

/// TOFU pin store: first observation is trusted and stored; later observations must match
/// or the file is updated only when `allow_rotation` is granted (grace window policy).
pub fn pin_tofu(pin_path: &str, new_pem: &str, allow_rotation: bool) -> Result<PinDecision> {
    match std::fs::read_to_string(pin_path) {
        Ok(existing) => {
            let existing = existing.trim();
            if existing == new_pem.trim() {
                Ok(PinDecision::Unchanged)
            } else if allow_rotation {
                std::fs::write(pin_path, new_pem.trim())?;
                Ok(PinDecision::Rotated)
            } else {
                Ok(PinDecision::Mismatch)
            }
        }
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => {
            if let Some(parent) = std::path::Path::new(pin_path).parent() {
                std::fs::create_dir_all(parent).with_context(|| format!("creating {parent:?}"))?;
            }
            std::fs::write(pin_path, new_pem.trim())?;
            Ok(PinDecision::FirstPinned)
        }
        Err(e) => Err(e).context("reading pin store"),
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum PinDecision {
    FirstPinned,
    Unchanged,
    Rotated,
    Mismatch,
}

#[cfg(test)]
mod tests {
    use super::*;

    fn tmp(name: &str) -> String {
        format!(
            "{}/ems-pin-{}-{name}",
            std::env::temp_dir().display(),
            std::process::id()
        )
    }

    #[test]
    fn tofu_first_then_mismatch_then_rotation() {
        let path = tmp("tofu");
        let _ = std::fs::remove_file(&path);
        assert_eq!(
            pin_tofu(&path, "CA-PEM-1", false).unwrap(),
            PinDecision::FirstPinned
        );
        assert_eq!(
            pin_tofu(&path, "CA-PEM-1", false).unwrap(),
            PinDecision::Unchanged
        );
        assert_eq!(
            pin_tofu(&path, "CA-PEM-2", false).unwrap(),
            PinDecision::Mismatch
        );
        assert_eq!(
            pin_tofu(&path, "CA-PEM-2", true).unwrap(),
            PinDecision::Rotated
        );
        assert_eq!(
            pin_tofu(&path, "CA-PEM-2", false).unwrap(),
            PinDecision::Unchanged
        );
        let _ = std::fs::remove_file(&path);
    }

    // --- client_ca.pem is published base64-encoded (wellknown.schema.json:
    //     contentEncoding: base64); the fetcher must decode it before pinning.

    #[test]
    fn decodes_base64_encoded_pem() {
        let b64 = "LS0tLS1CRUdJTiBDRVJUSUZJQ0FURS0tLS0tCg=="; // base64("-----BEGIN CERTIFICATE-----\n")
        assert_eq!(
            decode_client_ca_pem(b64).unwrap(),
            "-----BEGIN CERTIFICATE-----\n"
        );
    }

    #[test]
    fn rejects_payload_that_is_not_base64() {
        assert!(decode_client_ca_pem("not base64 !!").is_err());
    }

    // --- TTL / rotation grace: not_before / not_after (RFC 3339) bound the pinned CA;
    //     fetching outside the window must fail — a silently expired pin is a rotation bug.

    #[test]
    fn ca_window_valid_now_is_ok() {
        // test cert validity: 2026-09-25 .. 2026-10-25 (UTC)
        assert!(validate_ca_window(
            Some("2026-09-25T00:00:00Z"),
            Some("2026-10-25T00:00:00Z"),
            time::OffsetDateTime::from_unix_timestamp(1_790_335_292).unwrap(), // 2026-09-25
        )
        .is_ok());
    }

    #[test]
    fn ca_window_absent_is_ok() {
        assert!(validate_ca_window(None, None, now_unix()).is_ok());
    }

    #[test]
    fn expired_ca_window_is_rejected() {
        assert!(validate_ca_window(
            Some("2026-01-01T00:00:00Z"),
            Some("2026-02-01T00:00:00Z"),
            now_unix(),
        )
        .is_err());
    }

    #[test]
    fn not_yet_valid_ca_window_is_rejected() {
        assert!(validate_ca_window(
            Some("2099-01-01T00:00:00Z"),
            Some("2099-12-31T00:00:00Z"),
            now_unix(),
        )
        .is_err());
    }

    #[test]
    fn unparseable_ca_window_is_rejected() {
        assert!(validate_ca_window(Some("yesterday"), None, now_unix()).is_err());
    }
}
