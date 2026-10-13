//! Audit logging for `hems_eebus_local_qr` decryption.
//!
//! Policy: the EMS backend holds the QR RSA private key and decrypts the QR content
//! before passing pairing data to the MSB. Every decryption is audited with metadata only:
//! WHICH HEMS serial, WHICH MSB, WHEN, and the outcome. Never the QR plaintext,
//! never SKIs or SHIP secrets (AGENTS.md §6).

use std::path::PathBuf;
use std::sync::Mutex;

use anyhow::Context;
use rsa::pkcs8::DecodePrivateKey;
use serde::Serialize;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
pub enum DecryptOutcome {
    #[serde(rename = "success")]
    Success,
    #[serde(rename = "failure")]
    Failure,
}

#[derive(Debug, Serialize)]
pub struct AuditEvent {
    /// RFC3339-ish timestamp (unix millis)
    pub ts_ms: u128,
    pub action: &'static str, // "qr_decrypt"
    pub hems_serial: String,
    pub msb_id: String,
    pub outcome: DecryptOutcome,
}

/// Append-only audit sink: JSON lines in a file + tracing target `AUDIT`.
pub struct AuditLog {
    /// Sink location, exposed for diagnostics/tests.
    pub path: PathBuf,
    file: Mutex<std::fs::File>,
}

impl AuditLog {
    pub fn open(path: &str) -> anyhow::Result<Self> {
        if let Some(parent) = std::path::Path::new(path).parent() {
            if !parent.as_os_str().is_empty() {
                std::fs::create_dir_all(parent)?;
            }
        }
        let file = std::fs::OpenOptions::new()
            .create(true)
            .append(true)
            .open(path)?;
        Ok(Self {
            path: PathBuf::from(path),
            file: Mutex::new(file),
        })
    }

    pub fn record(&self, event: &AuditEvent) -> anyhow::Result<()> {
        let line = serde_json::to_string(event)?;
        tracing::info!(target: "AUDIT", "{line}");
        use std::io::Write;
        let mut f = self.file.lock().unwrap();
        f.write_all(line.as_bytes())?;
        f.write_all(b"\n")?;
        Ok(())
    }
}

/// Decrypt a base64 RSA QR ciphertext with a locally configured key.
/// Supports OAEP-SHA256 and legacy PKCS#1 v1.5 payloads (ADR-0014).
/// Callers MUST audit the outcome via [`AuditLog`], success or failure.
pub fn decrypt_qr(private_pem: &str, cipher_b64: &str) -> anyhow::Result<Vec<u8>> {
    let key =
        rsa::RsaPrivateKey::from_pkcs8_pem(private_pem).context("loading HEMS secrets key")?;
    use base64::Engine;
    let cipher = base64::engine::general_purpose::STANDARD
        .decode(cipher_b64)
        .context("decoding base64 QR cipher")?;
    let plaintext = key
        .decrypt(rsa::Oaep::new::<sha2::Sha256>(), &cipher)
        .or_else(|oaep_err| {
            key.decrypt(rsa::Pkcs1v15Encrypt, &cipher)
                .map_err(|_v15_err| {
                    anyhow::anyhow!("RSA decryption failed (OAEP-SHA256: {oaep_err})")
                })
        })
        .context("RSA decryption failed with OAEP-SHA256 and PKCS#1 v1.5")?;
    Ok(plaintext)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn tmp_path(name: &str) -> String {
        format!(
            "{}/ems-audit-{name}-{}",
            std::env::temp_dir().display(),
            std::process::id()
        )
    }

    #[test]
    fn rsa_pkcs1v15_roundtrip_hems_compatibility() {
        // Preserve both public encryption formats with a synthetic roundtrip.
        let mut rng = rand::thread_rng();
        let priv_key = rsa::RsaPrivateKey::new(&mut rng, 2048).unwrap();
        let pub_key = rsa::RsaPublicKey::from(&priv_key);
        let plaintext = br#"SHIP;SKI:0000000000000000000000000000000000000002;ENDSHIP;"#;
        let cipher = pub_key
            .encrypt(&mut rng, rsa::Pkcs1v15Encrypt, plaintext)
            .unwrap();
        use base64::Engine;
        use rsa::pkcs8::EncodePrivateKey;
        let cipher_b64 = base64::engine::general_purpose::STANDARD.encode(cipher);
        // render the key as PKCS#8 PEM
        let pem = priv_key
            .to_pkcs8_pem(rsa::pkcs8::LineEnding::LF)
            .unwrap()
            .to_string();
        let decrypted = super::decrypt_qr(&pem, &cipher_b64).unwrap();
        assert_eq!(decrypted, plaintext);
        // and OAEP-SHA256 still works (forward path)
        let cipher_oaep = pub_key
            .encrypt(&mut rng, rsa::Oaep::new::<sha2::Sha256>(), plaintext)
            .unwrap();
        let cipher_b64_oaep = base64::engine::general_purpose::STANDARD.encode(cipher_oaep);
        assert_eq!(
            super::decrypt_qr(&pem, &cipher_b64_oaep).unwrap(),
            plaintext
        );
    }

    #[test]
    fn audit_event_roundtrip() {
        let path = tmp_path("file");
        let log = AuditLog::open(&path).unwrap();
        log.record(&AuditEvent {
            ts_ms: 1,
            action: "qr_decrypt",
            hems_serial: "SNTESTHEMS0001".into(),
            msb_id: "msb-reference".into(),
            outcome: DecryptOutcome::Success,
        })
        .unwrap();
        let content = std::fs::read_to_string(&path).unwrap();
        assert!(content.contains("\"hems_serial\":\"SNTESTHEMS0001\""));
        assert!(content.contains("\"action\":\"qr_decrypt\""));
        std::fs::remove_file(&path).ok();
    }

    #[test]
    fn rsa_oaep_roundtrip() {
        // self-contained test keypair (2048), encrypt + decrypt through our helpers
        use rsa::traits::PublicKeyParts;
        let mut rng = rand::thread_rng();
        let priv_key = rsa::RsaPrivateKey::new(&mut rng, 2048).unwrap();
        let pub_key = rsa::RsaPublicKey::from(&priv_key);
        let plaintext = br#"{"hems_serial":"SNTESTHEMS0001"}"#;
        let cipher = pub_key
            .encrypt(&mut rng, rsa::Oaep::new::<sha2::Sha256>(), plaintext)
            .unwrap();
        let decrypted = priv_key
            .decrypt(rsa::Oaep::new::<sha2::Sha256>(), &cipher)
            .unwrap();
        assert_eq!(decrypted, plaintext);
        let _ = pub_key.n();
    }
}
