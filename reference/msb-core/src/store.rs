//! Portfolio store, provisioning records (write-once / idempotency), kill switch,
//! and per-client rate limiting.
//!
//! [`MsbStore`] is the integration seam for an MSB's real portfolio/inventory DB.
//! [`InMemoryStore`] is the reference dummy behind the `msb` binary and e2e. The
//! semantic contract of every method is normative (AGENTS.md §6, ADR-0004/-0005/
//! -0006/-0007/-0012) and validated by the conformance suite — a custom
//! implementation MUST uphold it.

use std::collections::{HashMap, HashSet};
use std::sync::Mutex;
use std::time::{Duration, Instant};

use async_trait::async_trait;
use serde::{Deserialize, Serialize};

use crate::matcher;

/// Inventory entry: expected box evidence per canonical serial.
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct InventoryEntry {
    pub ship_id: String,
    pub ski: String,
}

/// A stored pairing dataset (subset relevant to the reference server, ADR-0017):
/// ship_id + ski + SHIP-Info-Datensatz (qr_info carries the FPH inside the QR string).
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PairingRecord {
    pub ship_id: String,
    pub ski: String,
    /// SHIP-Info-Datensatz from the EEBUS QR content (`SHIP;…ENDSHIP;`).
    pub qr_info: String,
}

/// Normative integration seam: the MSB implements this against its real
/// portfolio/inventory DB and hands it to `api::api_router`. Every method's
/// semantics are normative and checked black-box by the conformance suite.
///
/// All methods take `&self`; implementations are expected to be interior-mutable
/// (DB connection pools, locks) and must be `Send + Sync`.
#[async_trait]
pub trait MsbStore: Send + Sync {
    /// Tolerant discovery lookup (ADR-0004/-0012): VPID form and `-suffix` forms
    /// resolve to the canonical portfolio serial. Anti-enumeration: a query
    /// shorter than a portfolio serial MUST return `None`.
    async fn lookup(&self, query: &str) -> Option<String>;

    /// Kill-switch state for a canonical serial (operator-controlled).
    /// Killed serials must be indistinguishable from unknown ones (no existence leak).
    async fn is_killed(&self, serial: &str) -> bool;

    /// Activate the kill switch for a canonical serial (admin channel: `KILL <serial>`).
    async fn kill(&self, serial: &str);

    /// Whether the canonical serial is in this MSB's portfolio. Kill switch and
    /// not-in-portfolio are both answered 404 upstream — no existence leak.
    async fn in_portfolio(&self, serial: &str) -> bool;

    /// Expected box evidence for a canonical serial (ADR-0007). `None` means
    /// inventory unknown: supplied evidence must not silently pass (operator alarm).
    async fn inventory(&self, serial: &str) -> Option<InventoryEntry>;

    /// Idempotent provisioning on `(serial, ship_id)` (ADR-0006): no record →
    /// `Created`, same ship_id → `Idempotent`, different ship_id →
    /// `WriteOnceConflict`. Write-once: a serial never changes its ship_id.
    async fn provision(&self, serial: &str, record: PairingRecord) -> ProvisionOutcome;
}

/// Reference dummy store: inventory in memory, provisioning idempotent on
/// `(serial, ship_id)`, kill switch in memory. Used by the `msb` binary,
/// the Compose stack and e2e; suitable as a template for DB-backed impls.
#[derive(Default)]
pub struct InMemoryStore {
    /// canonical serial -> expected box evidence
    pub inventory: HashMap<String, InventoryEntry>,
    /// canonical serial -> provisioned record (write-once on (serial, ship_id))
    records: Mutex<HashMap<String, PairingRecord>>,
    /// kill switch: serials disabled by the MSB operator
    killed: Mutex<HashSet<String>>,
}

impl InMemoryStore {
    pub fn new(inventory: HashMap<String, InventoryEntry>) -> Self {
        Self {
            inventory,
            records: Mutex::new(HashMap::new()),
            killed: Mutex::new(HashSet::new()),
        }
    }

    pub fn kill_sync(&self, serial: &str) {
        self.killed.lock().unwrap().insert(serial.to_string());
    }
}

#[async_trait]
impl MsbStore for InMemoryStore {
    async fn lookup(&self, query: &str) -> Option<String> {
        matcher::discover(&self.inventory.keys().cloned().collect::<Vec<_>>(), query)
            .map(str::to_string)
    }

    async fn is_killed(&self, serial: &str) -> bool {
        self.killed.lock().unwrap().contains(serial)
    }

    async fn kill(&self, serial: &str) {
        self.killed.lock().unwrap().insert(serial.to_string());
    }

    async fn in_portfolio(&self, serial: &str) -> bool {
        self.inventory.contains_key(serial)
    }

    async fn inventory(&self, serial: &str) -> Option<InventoryEntry> {
        self.inventory.get(serial).cloned()
    }

    async fn provision(&self, serial: &str, record: PairingRecord) -> ProvisionOutcome {
        let mut records = self.records.lock().unwrap();
        match records.get(serial) {
            Some(existing) if existing.ship_id == record.ship_id => ProvisionOutcome::Idempotent,
            Some(_) => ProvisionOutcome::WriteOnceConflict,
            None => {
                records.insert(serial.to_string(), record);
                ProvisionOutcome::Created
            }
        }
    }
}

#[derive(Debug, PartialEq)]
pub enum ProvisionOutcome {
    Created,
    Idempotent,
    WriteOnceConflict,
}

/// Per-client rate limiter (ADR-0010: 429 + Retry-After is contract). Lives in
/// core, not in [`MsbStore`] — an embedding MSB may replace it with its own
/// limiter, but the observable behavior MUST stay 429 + truthful Retry-After.
pub struct RateLimiter {
    limit: usize,
    window: Duration,
    state: Mutex<HashMap<String, (u32, Instant)>>,
}

impl RateLimiter {
    pub fn new(limit: usize) -> Self {
        Self {
            limit,
            window: Duration::from_secs(10),
            state: Mutex::new(HashMap::new()),
        }
    }

    /// Ok(()) or Err(remaining window in seconds) for one request from `client_id`.
    pub fn allow(&self, client_id: &str) -> Result<(), u64> {
        let mut state = self.state.lock().unwrap();
        let now = Instant::now();
        let entry = state.entry(client_id.to_string()).or_insert((0, now));
        let elapsed = now.duration_since(entry.1);
        if elapsed > self.window {
            *entry = (0, now);
        }
        if entry.0 >= self.limit as u32 {
            let remaining = self
                .window
                .checked_sub(elapsed)
                .map(|d| d.as_secs() + 1)
                .unwrap_or(1);
            return Err(remaining.max(1));
        }
        entry.0 += 1;
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn record(ship_id: &str) -> PairingRecord {
        PairingRecord {
            ship_id: ship_id.into(),
            ski: "ski".into(),
            qr_info: "SHIP;SKI:ski;ID:hems-id;FPH256:fph;ENDSHIP;".into(),
        }
    }

    async fn provision<S: MsbStore>(store: &S, serial: &str, ship_id: &str) -> ProvisionOutcome {
        store.provision(serial, record(ship_id)).await
    }

    #[tokio::test]
    async fn idempotent_on_same_ship_id() {
        let store = InMemoryStore::default();
        assert_eq!(
            provision(&store, "S1", "ship-a").await,
            ProvisionOutcome::Created
        );
        assert_eq!(
            provision(&store, "S1", "ship-a").await,
            ProvisionOutcome::Idempotent
        );
        assert_eq!(
            provision(&store, "S1", "ship-b").await,
            ProvisionOutcome::WriteOnceConflict
        );
    }

    #[tokio::test]
    async fn write_once_per_serial() {
        let store = InMemoryStore::default();
        provision(&store, "S1", "ship-a").await;
        provision(&store, "S2", "ship-a").await; // same ship_id, other serial → fine
        assert_eq!(
            provision(&store, "S2", "ship-c").await,
            ProvisionOutcome::WriteOnceConflict
        );
    }

    #[tokio::test]
    async fn lookup_is_tolerant_and_anti_enumeration() {
        let mut inventory = HashMap::new();
        inventory.insert("SNTEST0001".to_string(), InventoryEntry::default());
        let store = InMemoryStore::new(inventory);
        assert_eq!(store.lookup("SNTEST0001").await, Some("SNTEST0001".into()));
        assert_eq!(
            store.lookup("SNTEST0001-A1").await,
            Some("SNTEST0001".into())
        );
        assert_eq!(
            store.lookup("i:9901_u:SNTEST0001-X1").await,
            Some("SNTEST0001".into())
        );
        // anti-enumeration: prefix of a portfolio serial is a miss (404 on discovery)
        assert_eq!(store.lookup("SNTEST").await, None);
    }

    #[tokio::test]
    async fn rate_limit_blocks_after_threshold_with_truthful_retry_after() {
        let limiter = RateLimiter::new(3);
        assert!(limiter.allow("client-a").is_ok());
        assert!(limiter.allow("client-a").is_ok());
        assert!(limiter.allow("client-a").is_ok());
        let wait = limiter.allow("client-a").unwrap_err();
        assert!((1..=10).contains(&wait));
        assert!(limiter.allow("client-b").is_ok()); // other clients unaffected
    }

    #[tokio::test]
    async fn kill_switch_disables_serial() {
        let store = InMemoryStore::default();
        assert!(!store.is_killed("S1").await);
        store.kill("S1").await;
        assert!(store.is_killed("S1").await);
    }
}
