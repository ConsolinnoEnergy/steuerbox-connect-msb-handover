//! mTLS API: the two normative endpoints (`spec/discovery.yaml`, `spec/provisioning.yaml`),
//! the axum router, and the TLS serve loop.
//!
//! Embedding MSBs build [`MsbState`] with their own [`crate::store::MsbStore`] and
//! mount [`api_router`] (or call [`serve_tls`]) inside their existing backend —
//! mTLS terminates inside the application (ADR-0009).

use std::net::SocketAddr;
use std::path::PathBuf;
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::{Arc, RwLock};

use axum::extract::{Path, State};
use axum::http::StatusCode;
use axum::response::IntoResponse;
use axum::routing::{get, post};
use axum::{Json, Router};
use rustls::ServerConfig;
use serde_json::json;
use sha2::Digest;
use tokio::net::TcpListener;

use crate::admin;
use crate::matcher;
use crate::store::{InventoryEntry, MsbStore, PairingRecord, ProvisionOutcome, RateLimiter};
use crate::tls;

/// axum extension: SHA-256 fingerprint of the client cert (rate-limit key, log metadata).
#[derive(Clone)]
pub struct ClientFingerprint(pub String);

/// Shared state of the steuerbox-connect API. An embedding MSB fills
/// `store` with its DB-backed [`crate::store::MsbStore`] implementation.
pub struct MsbState {
    pub store: Arc<dyn MsbStore>,
    pub rate_limiter: RateLimiter,
    pub msb_id: String,
    /// IANA PEN of the pinned EMS manufacturer (well-known doc, ADR-0018 fleet
    /// binding). Provisioning `pairing_dataset.ship_id` must carry this PEN.
    pub iana_pen: String,
    pub box_admin: Arc<dyn admin::BoxAdminAdapter>,
    pub admin_token: String,
    pub tls_cert: PathBuf,
    pub tls_key: PathBuf,
    pub client_ca_dir: PathBuf,
    pub tls_cfg: RwLock<Option<Arc<ServerConfig>>>,
    pub pins_fp: AtomicU64,
}

impl MsbState {
    /// Build the state. TLS material is only needed by [`serve`]/[`serve_tls`] —
    /// an embedding MSB that mounts [`api_router`] behind its own mTLS server
    /// needs no certificate paths here.
    pub fn new(
        store: Arc<dyn MsbStore>,
        msb_id: impl Into<String>,
        rate_limit: usize,
        box_admin: Arc<dyn admin::BoxAdminAdapter>,
        admin_token: impl Into<String>,
    ) -> Self {
        Self {
            store,
            rate_limiter: RateLimiter::new(rate_limit),
            msb_id: msb_id.into(),
            iana_pen: String::new(),
            box_admin,
            admin_token: admin_token.into(),
            tls_cert: PathBuf::new(),
            tls_key: PathBuf::new(),
            client_ca_dir: PathBuf::new(),
            tls_cfg: RwLock::new(None),
            pins_fp: AtomicU64::new(0),
        }
    }

    /// Configure the IANA PEN fleet binding (ADR-0018): the PEN the MSB pinned from
    /// the manufacturer's well-known document. Provisioning payloads whose
    /// `pairing_dataset.ship_id` carries a different PEN are rejected `422
    /// fleet_mismatch`. An embedding MSB fills this from its pinned well-known doc.
    pub fn with_iana_pen(mut self, iana_pen: impl Into<String>) -> Self {
        self.iana_pen = iana_pen.into();
        self
    }

    /// Configure the mTLS material for [`serve`]/[`serve_tls`] (sim-LE server identity
    /// + client-CA pin store dir). Required before calling `serve`.
    pub fn with_tls(mut self, tls_cert: PathBuf, tls_key: PathBuf, client_ca_dir: PathBuf) -> Self {
        self.tls_cert = tls_cert;
        self.tls_key = tls_key;
        self.client_ca_dir = client_ca_dir;
        self
    }
}

fn error_body(code: &str, message: &str) -> Json<serde_json::Value> {
    Json(json!({ "error": code, "message": message }))
}

// --- mTLS API handlers ----------------------------------------------------

/// Discovery: status-code based — 200 = found, 404 = not found (empty bodies).
/// The canonical serial is the Regel-S base of the query (deterministic); the
/// server does not transmit it. `unknown` lives client-side (ADR-0005).
async fn discover(
    State(state): State<Arc<MsbState>>,
    axum::Extension(fp): axum::Extension<ClientFingerprint>,
    Path(serial): Path<String>,
) -> axum::response::Response {
    if let Err(wait) = state.rate_limiter.allow(&fp.0) {
        return rate_limited(wait);
    }
    match state.store.lookup(&serial).await {
        Some(canonical) if !state.store.is_killed(&canonical).await => {
            StatusCode::OK.into_response()
        }
        // unknown serial, anti-enumeration miss, and killed serial all read 404
        // (kill switch must not leak that the serial ever existed)
        _ => StatusCode::NOT_FOUND.into_response(),
    }
}

fn rate_limited(retry_after_secs: u64) -> axum::response::Response {
    (
        StatusCode::TOO_MANY_REQUESTS,
        [("Retry-After", retry_after_secs.to_string())],
        error_body("rate_limited", "quota exceeded; honor Retry-After"),
    )
        .into_response()
}

#[derive(serde::Deserialize)]
struct ProvisionBody {
    pairing_dataset: ProvisionDataset,
    box_evidence: Option<BoxEvidence>,
}

#[derive(serde::Deserialize)]
struct ProvisionDataset {
    ship_id: String,
    ski: String,
    qr_info: String,
}

/// qr_info MUST be a SHIP-Info-Datensatz (ADR-0017, pattern `^SHIP;.+ENDSHIP;$`).
fn is_ship_info_dataset(s: &str) -> bool {
    s.starts_with("SHIP;") && s.ends_with("ENDSHIP;") && s.len() > "SHIP;ENDSHIP;".len()
}

#[derive(serde::Deserialize)]
struct BoxEvidence {
    ship_id: Option<String>,
    ski: Option<String>,
}

/// Outcome of the optional box_evidence check (ADR-0007: mismatch → 422 + operator
/// alarm; supplied-but-unverifiable evidence must never pass silently).
enum EvidenceCheck {
    Ok,
    /// Evidence supplied but no inventory entry to verify against → accept, alarm.
    Unverifiable,
    Mismatch,
}

fn check_box_evidence(
    expected: Option<&InventoryEntry>,
    evidence: &Option<BoxEvidence>,
) -> EvidenceCheck {
    let Some(evidence) = evidence else {
        return EvidenceCheck::Ok; // server MUST accept without evidence
    };
    match expected {
        None => EvidenceCheck::Unverifiable,
        Some(expected) => {
            let mismatch = evidence.ship_id.as_deref() != Some(expected.ship_id.as_str())
                || evidence.ski.as_deref() != Some(expected.ski.as_str());
            if mismatch {
                EvidenceCheck::Mismatch
            } else {
                EvidenceCheck::Ok
            }
        }
    }
}

async fn provision(
    State(state): State<Arc<MsbState>>,
    axum::Extension(fp): axum::Extension<ClientFingerprint>,
    Path(serial): Path<String>,
    body: Result<Json<ProvisionBody>, axum::extract::rejection::JsonRejection>,
) -> axum::response::Response {
    if let Err(wait) = state.rate_limiter.allow(&fp.0) {
        return rate_limited(wait);
    }
    let Json(body) = match body {
        Ok(b) => b,
        Err(_) => {
            return (
                StatusCode::UNPROCESSABLE_ENTITY,
                error_body(
                    "invalid_body",
                    "request body is not a valid provisioning request",
                ),
            )
                .into_response()
        }
    };

    // Provisioning writes ONLY on the canonical serial — never on a client-derived one.
    if !matcher::is_canonical(&serial) || matcher::regel_s_base(&serial) != serial {
        return (
            StatusCode::BAD_REQUEST,
            error_body(
                "not_canonical",
                "provisioning requires the canonical serial from discovery",
            ),
        )
            .into_response();
    }
    if state.store.is_killed(&serial).await || !state.store.in_portfolio(&serial).await {
        // kill switch and not-in-portfolio are indistinguishable — no existence leak
        return (
            StatusCode::NOT_FOUND,
            error_body("not_found", "serial not in portfolio"),
        )
            .into_response();
    }

    // box_evidence: optional (MUST accept without) but verified when present;
    // supplied-but-unverifiable evidence gets an operator alarm, never a silent pass.
    let entry = state.store.inventory(&serial).await;
    match check_box_evidence(entry.as_ref(), &body.box_evidence) {
        EvidenceCheck::Ok => {}
        EvidenceCheck::Unverifiable => {
            tracing::warn!(
                serial,
                "box_evidence supplied but serial not in inventory — cannot verify, operator alarm (accepting)"
            );
        }
        EvidenceCheck::Mismatch => {
            tracing::warn!(
                serial,
                "box_evidence mismatch — operator alarm (422 box_mismatch)"
            );
            return (
                StatusCode::UNPROCESSABLE_ENTITY,
                error_body(
                    "box_mismatch",
                    "observed box evidence does not match inventory",
                ),
            )
                .into_response();
        }
    }

    let dataset = body.pairing_dataset;
    if dataset.ship_id.is_empty()
        || dataset.ski.is_empty()
        || !is_ship_info_dataset(&dataset.qr_info)
    {
        return (
            StatusCode::UNPROCESSABLE_ENTITY,
            error_body(
                "invalid_pairing_dataset",
                "pairing_dataset incomplete or invalid",
            ),
        )
            .into_response();
    }

    // Fleet binding (ADR-0018): the IANA PEN inside pairing_dataset.ship_id
    // (`i:<PEN>_u:<VPID>`) must match the iana_pen the MSB pinned from the
    // manufacturer's well-known document — an MSB never accepts foreign-fleet
    // pairing data. Mismatch → 422 + operator alarm, never a silent ignore.
    if !state.iana_pen.is_empty() {
        match matcher::iana_pen(&dataset.ship_id) {
            Some(pen) if pen == state.iana_pen => {}
            _ => {
                tracing::warn!(
                    serial,
                    "ship_id IANA PEN does not match well-known iana_pen — operator alarm (422 fleet_mismatch)"
                );
                return (
                    StatusCode::UNPROCESSABLE_ENTITY,
                    error_body(
                        "fleet_mismatch",
                        "ship_id belongs to a different manufacturer fleet",
                    ),
                )
                    .into_response();
            }
        }
    }

    let record = PairingRecord {
        ship_id: dataset.ship_id,
        ski: dataset.ski,
        qr_info: dataset.qr_info,
    };

    match state.store.provision(&serial, record.clone()).await {
        ProvisionOutcome::WriteOnceConflict => (
            StatusCode::CONFLICT,
            error_body(
                "write_once_conflict",
                "serial is already paired to a different ship_id",
            ),
        )
            .into_response(),
        ProvisionOutcome::Created => {
            // push onto the box via the admin channel (async; the closed loop proves delivery)
            let admin = state.box_admin.clone();
            tokio::spawn(async move {
                if let Err(e) = admin.write_pairing(&serial, &record).await {
                    tracing::warn!(serial, error = %e, "admin-channel write failed");
                }
            });
            (StatusCode::ACCEPTED, Json(json!({ "status": "accepted" }))).into_response()
        }
        ProvisionOutcome::Idempotent => {
            (StatusCode::ACCEPTED, Json(json!({ "status": "accepted" }))).into_response()
        }
    }
}

// --- servers ---------------------------------------------------------------

struct Svc {
    app: Router,
    fingerprint: String,
}

impl hyper::service::Service<hyper::Request<hyper::body::Incoming>> for Svc {
    type Response = axum::response::Response;
    type Error = std::convert::Infallible;
    type Future = std::pin::Pin<
        Box<dyn std::future::Future<Output = Result<Self::Response, Self::Error>> + Send>,
    >;

    fn call(&self, mut req: hyper::Request<hyper::body::Incoming>) -> Self::Future {
        let app = self.app.clone();
        let fp = ClientFingerprint(self.fingerprint.clone());
        Box::pin(async move {
            use tower::ServiceExt;
            req.extensions_mut().insert(fp);
            app.oneshot(req).await
        })
    }
}

async fn serve_tls(app: Router, listener: TcpListener, state: Arc<MsbState>) -> anyhow::Result<()> {
    loop {
        let (tcp, peer) = listener.accept().await?;
        let app = app.clone();
        let state = state.clone();
        tokio::spawn(async move {
            // rebuild TLS config when the client-CA pin store rotated (rotation grace);
            // None on the first connection → build once
            let fp = tls::pins_fingerprint(&state.client_ca_dir).unwrap_or(0);
            if fp != state.pins_fp.swap(fp, Ordering::Relaxed)
                || state.tls_cfg.read().unwrap().is_none()
            {
                match tls::build_mtls_config(&state.tls_cert, &state.tls_key, &state.client_ca_dir)
                {
                    Ok(cfg) => *state.tls_cfg.write().unwrap() = Some(Arc::new(cfg)),
                    Err(e) => {
                        tracing::warn!(error = %e, "pin store reload failed; keeping previous")
                    }
                }
            }
            let Some(cfg) = state.tls_cfg.read().unwrap().clone() else {
                tracing::warn!("no mTLS config available — dropping connection");
                return;
            };
            let conn = match tokio_rustls::TlsAcceptor::from(cfg).accept(tcp).await {
                Ok(c) => c,
                Err(e) => {
                    tracing::debug!(peer = %peer, error = %e, "TLS handshake failed (rejected)");
                    return;
                }
            };
            let fingerprint = conn
                .get_ref()
                .1
                .peer_certificates()
                .and_then(|certs| certs.first())
                .map(|c| hex::encode(sha2::Sha256::digest(c.as_ref())))
                .unwrap_or_default();

            let _ = hyper::server::conn::http1::Builder::new()
                .serve_connection(hyper_util::rt::TokioIo::new(conn), Svc { app, fingerprint })
                .await;
        });
    }
}

async fn serve_admin(state: Arc<MsbState>, listener: TcpListener) -> anyhow::Result<()> {
    use tokio::io::{AsyncReadExt, AsyncWriteExt};
    loop {
        let (mut tcp, _) = listener.accept().await?;
        let state = state.clone();
        tokio::spawn(async move {
            // line protocol: line 1 = token, line 2 = `KILL <serial>`
            let mut buf = vec![0u8; 4096];
            let n =
                match tokio::time::timeout(std::time::Duration::from_secs(5), tcp.read(&mut buf))
                    .await
                {
                    Ok(Ok(n)) if n > 0 => n,
                    _ => return,
                };
            let text = String::from_utf8_lossy(&buf[..n]);
            let mut lines = text.lines();
            let token = lines.next().unwrap_or("").trim();
            let cmd = lines.next().unwrap_or("").trim();
            let reply = if token != state.admin_token {
                "401 unauthorized\n".to_string()
            } else if let Some(serial) = cmd.strip_prefix("KILL ") {
                state.store.kill(serial.trim()).await;
                tracing::warn!(serial = serial.trim(), "kill switch activated");
                "200 killed\n".to_string()
            } else {
                "400 unknown command\n".to_string()
            };
            let _ = tcp.write_all(reply.as_bytes()).await;
            let _ = tcp.shutdown().await;
        });
    }
}

/// Build the normative mTLS API router for the given state (two endpoints).
pub fn api_router(state: Arc<MsbState>) -> Router {
    Router::new()
        .route("/v1/serials/{serial}", get(discover))
        .route("/v1/serials/{serial}/provisioning", post(provision))
        .with_state(state)
}

/// Convenience: run the mTLS API + admin channel on the given binds (used by the
/// `msb` binary; an embedding MSB may instead mount `api_router` into its own server).
pub async fn serve(
    state: Arc<MsbState>,
    api_bind: SocketAddr,
    admin_bind: SocketAddr,
) -> anyhow::Result<()> {
    let api = TcpListener::bind(api_bind).await?;
    let admin = TcpListener::bind(admin_bind).await?;
    tracing::info!(api = %api_bind, admin = %admin_bind, "msb listening (mTLS-only API)");
    let admin_state = state.clone();
    tokio::spawn(serve_admin(admin_state, admin));
    serve_tls(api_router(state.clone()), api, state).await
}
#[cfg(test)]
mod qr_info_tests {
    use super::*;

    #[test]
    fn valid_ship_info_dataset() {
        assert!(is_ship_info_dataset(
            "SHIP;SKI:abc;ID:hems;FPH256:fph;ENDSHIP;"
        ));
    }

    #[test]
    fn malformed_qr_info_is_invalid_pairing_dataset() {
        assert!(!is_ship_info_dataset("")); // empty → 422
        assert!(!is_ship_info_dataset("SHIP;SKI:x;")); // no ENDSHIP
        assert!(!is_ship_info_dataset("NOTSHIP;SKI:x;ENDSHIP;")); // no prefix
        assert!(!is_ship_info_dataset("SHIP;ENDSHIP;")); // no fields
    }
}

#[cfg(test)]
mod box_evidence_tests {
    use super::*;

    fn entry() -> InventoryEntry {
        InventoryEntry {
            ship_id: "ship-x".into(),
            ski: "ski-x".into(),
        }
    }

    fn evidence(ship_id: Option<&str>, ski: Option<&str>) -> Option<BoxEvidence> {
        Some(BoxEvidence {
            ship_id: ship_id.map(str::to_string),
            ski: ski.map(str::to_string),
        })
    }

    #[test]
    fn no_evidence_is_accepted() {
        assert!(matches!(
            check_box_evidence(Some(&entry()), &None),
            EvidenceCheck::Ok
        ));
    }

    #[test]
    fn matching_evidence_is_ok() {
        assert!(matches!(
            check_box_evidence(Some(&entry()), &evidence(Some("ship-x"), Some("ski-x"))),
            EvidenceCheck::Ok
        ));
    }

    #[test]
    fn partial_evidence_is_mismatch() {
        assert!(matches!(
            check_box_evidence(Some(&entry()), &evidence(Some("ship-x"), None)),
            EvidenceCheck::Mismatch
        ));
        assert!(matches!(
            check_box_evidence(Some(&entry()), &evidence(None, Some("ski-x"))),
            EvidenceCheck::Mismatch
        ));
    }

    #[test]
    fn wrong_evidence_is_mismatch() {
        assert!(matches!(
            check_box_evidence(Some(&entry()), &evidence(Some("ship-y"), Some("ski-x"))),
            EvidenceCheck::Mismatch
        ));
    }

    // ADR-0007: "never a silent ignore" — supplied evidence without an inventory entry
    // must surface an operator alarm (Unverifiable), not pass silently.
    #[test]
    fn evidence_without_inventory_is_unverifiable() {
        assert!(matches!(
            check_box_evidence(None, &evidence(Some("ship-x"), Some("ski-x"))),
            EvidenceCheck::Unverifiable
        ));
    }
}

#[cfg(test)]
mod embed_tests {
    use std::collections::HashMap;

    use super::*;
    use crate::store::MsbStore;
    use tower::ServiceExt;

    /// Simulates an MSB bringing its own portfolio DB: implements [`MsbStore`]
    /// over a plain HashMap — the exact seam a real MSB fills with its DB.
    #[derive(Default)]
    struct CustomDbStore {
        portfolio: HashMap<String, InventoryEntry>,
        records: tokio::sync::Mutex<HashMap<String, PairingRecord>>,
    }

    #[async_trait::async_trait]
    impl MsbStore for CustomDbStore {
        async fn lookup(&self, query: &str) -> Option<String> {
            let candidate = matcher::regel_s_base(matcher::vpid_to_serial(query.trim()));
            self.portfolio
                .keys()
                .find(|s| s.as_str() == candidate)
                .cloned()
        }
        async fn is_killed(&self, _serial: &str) -> bool {
            false
        }
        async fn kill(&self, _serial: &str) {}
        async fn in_portfolio(&self, serial: &str) -> bool {
            self.portfolio.contains_key(serial)
        }
        async fn inventory(&self, serial: &str) -> Option<InventoryEntry> {
            self.portfolio.get(serial).cloned()
        }
        async fn provision(&self, serial: &str, record: PairingRecord) -> ProvisionOutcome {
            let mut records = self.records.lock().await;
            match records.get(serial) {
                Some(existing) if existing.ship_id == record.ship_id => {
                    ProvisionOutcome::Idempotent
                }
                Some(_) => ProvisionOutcome::WriteOnceConflict,
                None => {
                    records.insert(serial.to_string(), record);
                    ProvisionOutcome::Created
                }
            }
        }
    }

    fn state_with_custom_db() -> MsbState {
        let mut portfolio = HashMap::new();
        portfolio.insert(
            "SNTEST0001".to_string(),
            InventoryEntry {
                ship_id: "ship-x".into(),
                ski: "ski-x".into(),
            },
        );
        MsbState::new(
            Arc::new(CustomDbStore {
                portfolio,
                records: Default::default(),
            }),
            "msb-embedder",
            1000,
            Arc::new(admin::DisabledBoxAdmin),
            "tok",
        )
    }

    #[tokio::test]
    async fn custom_store_discovery_and_provisioning_through_router() {
        // no TLS material needed at router level — mTLS-only applies to serve();
        // an embedding MSB terminates mTLS in its own server or uses serve().
        let app = api_router(Arc::new(state_with_custom_db()));

        // the mTLS serve layer injects the client-cert fingerprint extension;
        // at router level the test injects it the same way
        let req = |method: &str, uri: &str, body: axum::body::Body| {
            let mut req = axum::http::Request::builder()
                .method(method)
                .uri(uri)
                .header("content-type", "application/json")
                .body(body)
                .unwrap();
            req.extensions_mut()
                .insert(ClientFingerprint("testfp".into()));
            req
        };

        let res = app
            .clone()
            .oneshot(req(
                "GET",
                "/v1/serials/SNTEST0001",
                axum::body::Body::empty(),
            ))
            .await
            .unwrap();
        // status-code based discovery: 200 = found (empty body)
        assert_eq!(res.status(), StatusCode::OK);
        let bytes = axum::body::to_bytes(res.into_body(), 1024).await.unwrap();
        assert!(bytes.is_empty(), "discovery 200 must carry no body");

        // 404 for unknown serials (empty body, no existence leak)
        let res = app
            .clone()
            .oneshot(req(
                "GET",
                "/v1/serials/SNTEST9999",
                axum::body::Body::empty(),
            ))
            .await
            .unwrap();
        assert_eq!(res.status(), StatusCode::NOT_FOUND);

        let res = app
            .oneshot(req(
                "POST",
                "/v1/serials/SNTEST0001/provisioning",
                axum::body::Body::from(
                    serde_json::to_vec(&json!({
                        "pairing_dataset": {
                            "ship_id": "i:0002_u:HEMS0001",
                            "ski": "ski-hems",
                            "qr_info": "SHIP;SKI:ski-hems;ID:hems-id;FPH256:fph-hems;ENDSHIP;"
                        },
                        "box_evidence": {"ship_id": "ship-x", "ski": "ski-x"}
                    }))
                    .unwrap(),
                ),
            ))
            .await
            .unwrap();
        assert_eq!(res.status(), StatusCode::ACCEPTED);
    }

    // ADR-0018 fleet binding: the IANA PEN inside pairing_dataset.ship_id must
    // match the well-known iana_pen — foreign-fleet payloads are 422 fleet_mismatch.
    #[tokio::test]
    async fn fleet_binding_rejects_foreign_iana_pen() {
        let app = api_router(Arc::new(state_with_custom_db().with_iana_pen("12345")));
        let req = |ship_id: &str| {
            let mut r = axum::http::Request::builder()
                .method("POST")
                .uri("/v1/serials/SNTEST0001/provisioning")
                .header("content-type", "application/json")
                .body(axum::body::Body::from(
                    serde_json::to_vec(&json!({
                        "pairing_dataset": {
                            "ship_id": ship_id,
                            "ski": "ski-hems",
                            "qr_info": "SHIP;SKI:ski-hems;ID:hems-id;FPH256:fph;ENDSHIP;"
                        },
                        "box_evidence": {"ship_id": "ship-x", "ski": "ski-x"}
                    }))
                    .unwrap(),
                ))
                .unwrap();
            r.extensions_mut()
                .insert(ClientFingerprint("testfp".into()));
            r
        };

        // foreign PEN → 422 fleet_mismatch + operator alarm
        let res = app.clone().oneshot(req("i:0002_u:HEMS0001")).await.unwrap();
        assert_eq!(res.status(), StatusCode::UNPROCESSABLE_ENTITY);
        let bytes = axum::body::to_bytes(res.into_body(), 1024).await.unwrap();
        let body: serde_json::Value = serde_json::from_slice(&bytes).unwrap();
        assert_eq!(body["error"], "fleet_mismatch");

        // own fleet PEN → accepted
        let res = app
            .clone()
            .oneshot(req("i:12345_u:HEMS0001"))
            .await
            .unwrap();
        assert_eq!(res.status(), StatusCode::ACCEPTED);

        // ship_id outside the SHIP-ID shape → fleet_mismatch too (never silent)
        let res = app.oneshot(req("ship-without-pen")).await.unwrap();
        assert_eq!(res.status(), StatusCode::UNPROCESSABLE_ENTITY);
    }
}
