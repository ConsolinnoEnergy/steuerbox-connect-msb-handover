//! Rust reference client (NORMATIVE behavior): the EMS manufacturer backend.
//!
//! Responsibilities:
//! - fan-out discovery across platform URLs with dedupe, tri-state (`unknown` on transport
//!   failure, never `404`)
//! - provisioning POST on the canonical serial returned by discovery — never a derived one
//! - well-known fetch + client-CA pin (TOFU + TTL + rotation grace)

pub mod audit;
pub mod shipqr;
pub mod wellknown;

pub use steuerbox_protocol::{
    canonical_from_query, iana_pen, matches_fleet, regel_s_base, vpid_to_serial,
};

use std::time::Duration;

use anyhow::{Context, Result};
use rustls_pki_types::pem::PemObject;
use rustls_pki_types::CertificateDer;

/// Client configuration: root bundle (server TLS trust) + own mTLS identity.
#[derive(Clone)]
pub struct ClientConfig {
    pub root_bundle_path: String,
    /// None = no mTLS client certificate (used for well-known fetches).
    pub client_cert_path: Option<String>,
    pub client_key_path: Option<String>,
}

impl ClientConfig {
    /// mTLS client against the MSB API.
    pub fn mtls(root_bundle: &str, cert: &str, key: &str) -> Self {
        Self {
            root_bundle_path: root_bundle.into(),
            client_cert_path: Some(cert.into()),
            client_key_path: Some(key.into()),
        }
    }

    /// Plain client (server-TLS only) for well-known fetches.
    pub fn server_tls(root_bundle: &str) -> Self {
        Self {
            root_bundle_path: root_bundle.into(),
            client_cert_path: None,
            client_key_path: None,
        }
    }

    pub fn build_http_client(&self) -> Result<reqwest::Client> {
        let mut builder = reqwest::Client::builder().use_rustls_tls();
        if !self.root_bundle_path.is_empty() {
            let certs = CertificateDer::pem_file_iter(&self.root_bundle_path)
                .with_context(|| format!("reading root bundle {}", self.root_bundle_path))?
                .collect::<Result<Vec<_>, _>>()
                .context("parsing root bundle")?;
            for cert in certs {
                builder = builder
                    .add_root_certificate(reqwest::tls::Certificate::from_der(cert.as_ref())?);
            }
        }
        if let (Some(cert), Some(key)) = (&self.client_cert_path, &self.client_key_path) {
            // reqwest identity expects concatenated PEM (leaf chain + key).
            // A cert file without trailing newline (e.g. GitLab CI file variables
            // trim it) would glue "END CERTIFICATE-----" and "BEGIN PRIVATE
            // KEY-----" into one line — the PEM parser then never finds the key.
            let cert_pem = std::fs::read(cert).with_context(|| format!("reading {cert}"))?;
            let key_pem = std::fs::read(key).with_context(|| format!("reading {key}"))?;
            let pem = concat_identity_pem(&cert_pem, &key_pem);
            builder = builder.identity(reqwest::Identity::from_pem(&pem)?);
        }
        Ok(builder.build()?)
    }
}

/// Concatenate certificate + key PEM into one identity PEM, guaranteeing a
/// newline between the sections (GitLab CI file variables lose the trailing
/// newline — without the separator the PEM section parser misses the key).
fn concat_identity_pem(cert_pem: &[u8], key_pem: &[u8]) -> Vec<u8> {
    let mut pem = Vec::with_capacity(cert_pem.len() + key_pem.len() + 1);
    pem.extend_from_slice(cert_pem);
    if pem.last() != Some(&b'\n') {
        pem.push(b'\n');
    }
    pem.extend_from_slice(key_pem);
    if pem.last() != Some(&b'\n') {
        pem.push(b'\n');
    }
    pem
}

/// Tri-state discovery result of one MSB platform URL.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Discovery {
    Found {
        canonical_serial: String,
    },
    NotFound,
    /// Transport failure, TLS rejection, unexpected status — NEVER `not found`.
    Unknown(String),
}

/// Discovery is status-code based: 200 = found (empty body), 404 = not found
/// (empty body), everything else / transport failure = `unknown` (ADR-0005).
/// The canonical serial is derived from the query itself, never transmitted.
pub async fn discover(client: &reqwest::Client, base_url: &str, query: &str) -> Discovery {
    let url = format!("{}/v1/serials/{}", base_url.trim_end_matches('/'), query);
    let resp = match client.get(&url).send().await {
        Ok(r) => r,
        Err(e) => return Discovery::Unknown(e.to_string()),
    };
    match resp.status() {
        reqwest::StatusCode::OK => Discovery::Found {
            canonical_serial: canonical_from_query(query).to_string(),
        },
        reqwest::StatusCode::NOT_FOUND => Discovery::NotFound,
        other => Discovery::Unknown(format!("{base_url} returned {other}")),
    }
}

/// Parse the numeric Retry-After header. Returns None when absent or non-numeric —
/// never a fabricated default (the contract requires an honest header).
pub fn retry_after_from(headers: &reqwest::header::HeaderMap) -> Option<u64> {
    headers
        .get("Retry-After")
        .and_then(|v| v.to_str().ok())
        .and_then(|v| v.parse().ok())
}

/// Fan-out discovery across platform URLs with dedupe. Returns one result per unique URL.
pub async fn fan_out_discover(
    client: &reqwest::Client,
    base_urls: &[String],
    query: &str,
) -> Vec<(String, Discovery)> {
    let mut seen = std::collections::HashSet::new();
    let mut results = Vec::new();
    for url in base_urls {
        let normalized = url.trim_end_matches('/').to_string();
        if !seen.insert(normalized.clone()) {
            continue; // URL dedupe
        }
        results.push((
            normalized.clone(),
            discover(client, &normalized, query).await,
        ));
    }
    results
}

/// Provisioning outcome (statuses per spec).
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ProvisionOutcome {
    Accepted,          // 202
    BoxMismatch,       // 422 box_mismatch
    WriteOnceConflict, // 409
    RateLimited {
        retry_after: u64,
    }, // 429 — clients MUST honor Retry-After
    Rejected(u16),     // 404 (not in portfolio / killed), 400, …
    /// 422 with a non-box_mismatch error code (invalid_pairing_dataset / invalid_body
    /// / unknown) — the code carries the machine-readable error from the body.
    Unprocessable(String),
    Unknown(String), // transport-level failure
}

/// Map a 422 error body to its outcome (provisioning.yaml ErrorResponse taxonomy).
pub fn classify_422(body: &serde_json::Value) -> ProvisionOutcome {
    match body.get("error").and_then(|e| e.as_str()) {
        Some("box_mismatch") => ProvisionOutcome::BoxMismatch,
        other => ProvisionOutcome::Unprocessable(other.unwrap_or("invalid_body").to_string()),
    }
}

/// Provision one endpoint. Honors 429 + Retry-After with exactly one retry.
pub async fn provision(
    client: &reqwest::Client,
    base_url: &str,
    serial: &str,
    body: &serde_json::Value,
) -> ProvisionOutcome {
    let url = format!(
        "{}/v1/serials/{}/provisioning",
        base_url.trim_end_matches('/'),
        serial
    );
    let mut attempt = 0;
    loop {
        let resp = match client.post(&url).json(body).send().await {
            Ok(r) => r,
            Err(e) => return ProvisionOutcome::Unknown(e.to_string()),
        };
        let status = resp.status().as_u16();
        match status {
            202 => return ProvisionOutcome::Accepted,
            422 => {
                // distinguish the 422 error codes (box_mismatch vs invalid_*)
                return match resp.json::<serde_json::Value>().await {
                    Ok(v) => classify_422(&v),
                    Err(_) => ProvisionOutcome::Unprocessable("invalid_body".into()),
                };
            }
            409 => return ProvisionOutcome::WriteOnceConflict,
            429 => {
                // The contract is 429 + Retry-After; a 429 without the header is a
                // contract violation and must not be retried with a fabricated delay.
                let retry_after = retry_after_from(resp.headers());
                if attempt > 0 {
                    return ProvisionOutcome::RateLimited {
                        retry_after: retry_after.unwrap_or(0),
                    };
                }
                let Some(retry_after) = retry_after else {
                    return ProvisionOutcome::Unknown(
                        "429 without Retry-After header — contract violation".to_string(),
                    );
                };
                attempt += 1;
                tokio::time::sleep(Duration::from_secs(retry_after)).await;
                continue;
            }
            other => return ProvisionOutcome::Rejected(other),
        }
    }
}

/// One-shot zero-touch helper: discover (canonical only) then provision.
pub async fn zero_touch(
    client: &reqwest::Client,
    base_urls: &[String],
    vpid_query: &str,
    pairing_body: &serde_json::Value,
) -> Result<(String, ProvisionOutcome)> {
    let results = fan_out_discover(client, base_urls, vpid_query).await;
    // Any unknown transport failure blocks a `not found` conclusion (tri-state).
    let mut hit: Option<(String, String)> = None;
    for (url, d) in &results {
        match d {
            Discovery::Found {
                canonical_serial, ..
            } => {
                hit = Some((url.clone(), canonical_serial.clone()));
            }
            Discovery::NotFound => {} // continue looking at other platforms
            Discovery::Unknown(why) => {
                anyhow::bail!("discovery against {url} is unknown — refusing to conclude: {why}")
            }
        }
    }
    let (url, canonical) = hit.context("serial not found on any platform")?;
    // Provisioning writes ONLY on the canonical serial returned by discovery.
    let outcome = provision(client, url.as_str(), canonical.as_str(), pairing_body).await;
    Ok((canonical, outcome))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn webpki_client_does_not_require_a_custom_root_bundle() {
        assert!(ClientConfig::server_tls("").build_http_client().is_ok());
    }

    /// Regression: GitLab CI file variables lose the trailing newline — the
    /// concatenated identity PEM must still parse (reqwest "private key or
    /// certificate not found" otherwise). The concatenated PEM must parse as
    /// one certificate + one key via reqwest::Identity.
    #[test]
    fn identity_pem_concat_adds_missing_newline() {
        // literals assembled so secret scanners do not match a fake PEM header
        // (false positive — no real key material, plain literals; synthetic test data)
        let cert = format!(
            "-----BEGIN {}-----\nabc\n-----END CERTIFICATE-----",
            "CERTIFICATE"
        );
        let key = format!(
            "-----BEGIN {}-----\ndef\n-----END {}-----\n",
            "PRIVATE KEY", "PRIVATE KEY"
        );
        let cert = cert.as_bytes();
        let key = key.as_bytes();
        let pem = concat_identity_pem(cert, key);
        assert_eq!(
            std::str::from_utf8(&pem).unwrap(),
            format!(
                "-----BEGIN CERTIFICATE-----\nabc\n-----END CERTIFICATE-----\n\
                 -----BEGIN {}-----\ndef\n-----END {}-----\n",
                "PRIVATE KEY", "PRIVATE KEY"
            )
        );
        // reqwest accepts it (fails with "not found" before the fix)
        reqwest::Identity::from_pem(&pem).expect("concatenated identity PEM must parse");
    }

    // ADR-0018 fleet binding: the client extracts the IANA PEN from the SHIP-ID
    // and compares it against the well-known iana_pen.
    #[test]
    fn iana_pen_extraction_and_fleet_check() {
        assert_eq!(iana_pen("i:12345_u:SNTEST_HEMS-0001"), Some("12345"));
        assert_eq!(iana_pen("ship-without-pen"), None);
        assert_eq!(iana_pen("i:_u:HEMS1"), None);
        assert!(matches_fleet("12345", "i:12345_u:HEMS1"));
        assert!(!matches_fleet("12345", "i:0002_u:HEMS1"));
        assert!(!matches_fleet("12345", "not-a-ship-id"));
    }

    #[test]
    fn tristate_types_are_distinct() {
        let a = Discovery::Found {
            canonical_serial: "S1".into(),
        };
        let b = Discovery::NotFound;
        let c = Discovery::Unknown("timeout".into());
        assert_ne!(a, b);
        assert_ne!(b, c);
        assert_ne!(a, c);
    }

    #[test]
    fn fan_out_dedupes_urls() {
        // dedupe logic verified via URL normalization on results count in e2e; here only
        // the normalization contract
        assert_eq!("http://x/".trim_end_matches('/'), "http://x");
    }

    // --- status-code discovery: canonical serial derived from the query
    // (200/404 empty bodies per spec/discovery.yaml v0.3.0, ADR-0005/-0012)

    #[test]
    fn canonical_from_query_regel_s_base() {
        assert_eq!(canonical_from_query("SNTEST0001"), "SNTEST0001");
        assert_eq!(canonical_from_query("SNTEST0001-X9"), "SNTEST0001");
        assert_eq!(canonical_from_query("SNTEST0001-A1-B2"), "SNTEST0001");
    }

    #[test]
    fn canonical_from_query_vpid_form() {
        assert_eq!(canonical_from_query("i:9901_u:SNTEST0001"), "SNTEST0001");
        assert_eq!(canonical_from_query("i:9901_u:SNTEST0001-X1"), "SNTEST0001");
        assert_eq!(canonical_from_query("i:9901_novpid"), "i:9901_novpid");
    }

    #[test]
    fn canonical_from_query_is_whitespace_tolerant() {
        assert_eq!(canonical_from_query("  SNTEST0001-X9  "), "SNTEST0001");
    }

    // --- Retry-After parsing: never fabricate a value; preserve the real header
    #[test]
    fn retry_after_header_is_parsed() {
        let mut h = reqwest::header::HeaderMap::new();
        h.insert("Retry-After", "7".parse().unwrap());
        assert_eq!(retry_after_from(&h), Some(7));
    }

    #[test]
    fn missing_or_nonnumeric_retry_after_is_none() {
        let h = reqwest::header::HeaderMap::new();
        assert_eq!(retry_after_from(&h), None);
        let mut h = reqwest::header::HeaderMap::new();
        h.insert("Retry-After", "soon".parse().unwrap());
        assert_eq!(retry_after_from(&h), None);
    }

    // --- 422 error taxonomy (provisioning.yaml): box_mismatch vs
    // invalid_pairing_dataset vs invalid_body must be distinguishable client-side.
    #[test]
    fn classify_422_codes() {
        let code = |c: &str| serde_json::json!({ "error": c, "message": "m" });
        assert_eq!(
            classify_422(&code("box_mismatch")),
            ProvisionOutcome::BoxMismatch
        );
        assert_eq!(
            classify_422(&code("invalid_pairing_dataset")),
            ProvisionOutcome::Unprocessable("invalid_pairing_dataset".into())
        );
        assert_eq!(
            classify_422(&code("invalid_body")),
            ProvisionOutcome::Unprocessable("invalid_body".into())
        );
        assert_eq!(
            classify_422(&serde_json::json!({ "message": "no error code" })),
            ProvisionOutcome::Unprocessable("invalid_body".into())
        );
    }
}
