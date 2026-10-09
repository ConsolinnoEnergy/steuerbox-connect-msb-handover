//! Admin-channel adapter: the MSB writes the pairing dataset onto the Steuerbox via its
//! existing admin interface. Only the MSB has the admin token (network boundary, AGENTS §6).
//! The trait exists for unit testing; production/compose use the HTTP adapter.

use crate::store::PairingRecord;
use anyhow::Result;

/// Where the MSB pushes pairing data for a box.
#[async_trait::async_trait]
pub trait BoxAdminAdapter: Send + Sync {
    /// Write the pairing dataset onto the box with canonical serial `serial`.
    async fn write_pairing(&self, serial: &str, record: &PairingRecord) -> Result<()>;
}

/// HTTP adapter against the box-mock admin receiver (local: plain HTTP on the ops plane).
pub struct HttpBoxAdmin {
    pub base_url: String,
    pub token: String,
    pub client: reqwest::Client,
}

#[async_trait::async_trait]
impl BoxAdminAdapter for HttpBoxAdmin {
    async fn write_pairing(&self, serial: &str, record: &PairingRecord) -> Result<()> {
        let url = format!(
            "{}/admin/pairing/{}",
            self.base_url.trim_end_matches('/'),
            serial
        );
        let resp = self
            .client
            .post(url)
            .header("x-admin-token", &self.token)
            .json(record)
            .send()
            .await?;
        let status = resp.status();
        let body = resp.text().await.unwrap_or_default();
        anyhow::ensure!(
            status.is_success(),
            "box admin write failed: {status} {body}"
        );
        Ok(())
    }
}

/// No-op adapter used when the MSB runs without a reachable box (still accepts
/// provisioning; the closed loop is proven by the SHIP handshake, not by the write).
pub struct DisabledBoxAdmin;

#[async_trait::async_trait]
impl BoxAdminAdapter for DisabledBoxAdmin {
    async fn write_pairing(&self, _serial: &str, _record: &PairingRecord) -> Result<()> {
        tracing::warn!("box admin disabled — skipping write");
        Ok(())
    }
}
