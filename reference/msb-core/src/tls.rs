//! mTLS setup for the MSB: sim-LE server identity + pinned client-CA store.
//!
//! The pin store is a directory of client-CA PEM files (`MSB_CLIENT_CA_DIR`). Rotation
//! adds/removes files; the server rebuilds its verifier when the directory changes so
//! rotation grace works without a restart (ClientCARotation e2e).

use std::path::Path;
use std::sync::Arc;
use std::time::SystemTime;

use anyhow::{Context, Result};
use rustls::server::WebPkiClientVerifier;
use rustls::{RootCertStore, ServerConfig};
use rustls_pki_types::pem::PemObject;
use rustls_pki_types::{CertificateDer, PrivateKeyDer};

/// Build a server config that REQUIRES a client certificate chaining to one of the
/// pinned client CAs (mTLS-only — no anonymous mode, hard rule).
pub fn build_mtls_config(
    cert_path: &Path,
    key_path: &Path,
    pins_dir: &Path,
) -> Result<ServerConfig> {
    let roots = load_pins(pins_dir)?;
    let verifier = WebPkiClientVerifier::builder(Arc::new(roots)).build()?;
    let certs = CertificateDer::pem_file_iter(cert_path)
        .with_context(|| format!("reading {}", cert_path.display()))?
        .collect::<Result<Vec<_>, _>>()?;
    let key = PrivateKeyDer::from_pem_file(key_path)?;
    Ok(ServerConfig::builder()
        .with_client_cert_verifier(verifier)
        .with_single_cert(certs, key)?)
}

fn load_pins(pins_dir: &Path) -> Result<RootCertStore> {
    let mut roots = RootCertStore::empty();
    let mut found = false;
    for entry in
        std::fs::read_dir(pins_dir).with_context(|| format!("pin store {}", pins_dir.display()))?
    {
        let entry = entry?;
        let path = entry.path();
        if path.extension().and_then(|e| e.to_str()) != Some("pem") {
            continue;
        }
        for cert in CertificateDer::pem_file_iter(&path)
            .with_context(|| format!("reading pin {}", path.display()))?
        {
            let cert = cert.with_context(|| format!("parsing pin {}", path.display()))?;
            roots.add(cert).context("adding pinned client CA")?;
            found = true;
        }
    }
    anyhow::ensure!(found, "client-CA pin store {} is empty", pins_dir.display());
    Ok(roots)
}

/// Fingerprint of the pin store (names + mtimes) to detect rotation without polling hooks.
pub fn pins_fingerprint(pins_dir: &Path) -> Option<u64> {
    let mut names: Vec<(String, SystemTime)> = Vec::new();
    for entry in std::fs::read_dir(pins_dir).ok()? {
        let entry = entry.ok()?;
        let meta = entry.metadata().ok()?;
        names.push((
            entry.file_name().to_string_lossy().into_owned(),
            meta.modified().ok()?,
        ));
    }
    names.sort();
    let mut hash: u64 = 0xcbf29ce484222325;
    for (name, mtime) in names {
        for byte in name.into_bytes() {
            hash = (hash ^ u64::from(byte)).wrapping_mul(0x100000001b3);
        }
        hash = (hash
            ^ mtime
                .duration_since(SystemTime::UNIX_EPOCH)
                .ok()?
                .as_nanos() as u64)
            .wrapping_mul(0x100000001b3);
    }
    Some(hash)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn pins_fingerprint_changes_on_mtime() {
        let dir = std::env::temp_dir().join(format!("msb-pins-test-{}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        let f = dir.join("ca.pem");
        std::fs::write(&f, "x").unwrap();
        let fp1 = pins_fingerprint(&dir);
        std::thread::sleep(std::time::Duration::from_millis(10));
        std::fs::write(&f, "yy").unwrap();
        let _ = std::fs::write(dir.join("ca2.pem"), "z");
        let fp2 = pins_fingerprint(&dir);
        assert!(fp1.is_some() && fp2.is_some());
        assert_ne!(fp1, fp2);
        std::fs::remove_dir_all(&dir).ok();
    }
}
