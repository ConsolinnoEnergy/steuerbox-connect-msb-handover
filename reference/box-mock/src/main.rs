//! Steuerbox stub (LAN side, reference stub — never a fake of our own components):
//! - admin receiver: pairing-dataset writes (token-gated; only the MSB has the token)
//! - SHIP-TLS listener: accepts the LAN handshake when the announced SKI matches the
//!   provisioned pairing dataset → closed-loop proof
//!
//! mDNS is NOT part of this stub: the assumption is that the reference HEMS has already
//! received everything from the Steuerbox via mDNS and forwarded it to the EMS backend
//! (`hems_eebus_stb_*`). The "SHIP handshake" is simulated: TLS to the box (server-TLS)
//! followed by one line `SKI:<hex>` — the box accepts only if it equals the provisioned
//! HEMS SKI.

use std::sync::{Arc, Mutex};

use axum::extract::{Path, State};
use axum::http::StatusCode;
use axum::response::IntoResponse;
use axum::routing::{get, post};
use axum::{Json, Router};
use serde::{Deserialize, Serialize};
use serde_json::json;

#[derive(Clone)]
struct BoxState {
    token: String,
    serial: String,
    inner: Arc<Mutex<BoxInner>>,
}

struct BoxInner {
    /// this box's own ship_id + ski (announced via mDNS TXT; box_evidence source)
    box_ship_id: String,
    box_ski: String,
    /// pairing dataset written by the MSB via the admin channel
    pairing: Option<PairingDataset>,
    /// closed-loop state: last SHIP handshake result
    ship: Option<ShipResult>,
}

#[derive(Clone, Serialize, Deserialize)]
struct PairingDataset {
    ship_id: String,
    ski: String,
    /// SHIP-Info-Datensatz from the EEBUS QR content (ADR-0017).
    #[serde(default)]
    qr_info: Option<String>,
}

#[derive(Clone, Serialize)]
struct ShipResult {
    ski: String,
    accepted: bool,
}

// --- admin receiver (token-gated; the MSB is the only authorized writer) -----------

async fn admin_pairing(
    State(state): State<BoxState>,
    Path(serial): Path<String>,
    headers: axum::http::HeaderMap,
    Json(dataset): Json<PairingDataset>,
) -> impl IntoResponse {
    if headers.get("x-admin-token").and_then(|v| v.to_str().ok()) != Some(state.token.as_str()) {
        return (
            StatusCode::UNAUTHORIZED,
            Json(json!({"error": "unauthorized"})),
        );
    }
    let mut inner = state.inner.lock().unwrap();
    match &inner.pairing {
        // write-once semantics on the box too
        Some(existing) if existing.ship_id != dataset.ship_id => (
            StatusCode::CONFLICT,
            Json(json!({"error": "write_once_conflict"})),
        ),
        Some(_) => (StatusCode::ACCEPTED, Json(json!({"status": "idempotent"}))),
        None => {
            tracing::info!(serial, ship_id = %dataset.ship_id, "pairing dataset written via admin channel");
            inner.pairing = Some(dataset);
            (StatusCode::OK, Json(json!({"status": "written"})))
        }
    }
}

async fn admin_status(State(state): State<BoxState>) -> impl IntoResponse {
    let inner = state.inner.lock().unwrap();
    Json(json!({
        "serial": state.serial,
        "box_ship_id": inner.box_ship_id,
        "box_ski": inner.box_ski,
        "pairing": inner.pairing,
        "ship": inner.ship,
    }))
}

// --- SHIP-TLS listener (SKI acceptance) --------------------------------------------

async fn ship_listener(state: BoxState, bind: std::net::SocketAddr) -> anyhow::Result<()> {
    let ck = rcgen::generate_simple_self_signed(vec!["steuerbox.local".into()])?;
    let certs = vec![rustls_pki_types::CertificateDer::from(ck.cert)];
    let key = rustls_pki_types::PrivateKeyDer::from(rustls_pki_types::PrivatePkcs8KeyDer::from(
        ck.key_pair.serialize_der(),
    ));
    let config = Arc::new(
        rustls::ServerConfig::builder()
            .with_no_client_auth()
            .with_single_cert(certs, key)?,
    );
    let listener = tokio::net::TcpListener::bind(bind).await?;
    tracing::info!(%bind, "box-mock SHIP listener up");
    loop {
        let (tcp, _) = listener.accept().await?;
        let state = state.clone();
        let config = config.clone();
        tokio::spawn(async move {
            let tls = match tokio_rustls::TlsAcceptor::from(config).accept(tcp).await {
                Ok(t) => t,
                Err(e) => {
                    tracing::debug!(error = %e, "SHIP TLS handshake failed");
                    return;
                }
            };
            ship_session(state, tls).await;
        });
    }
}

/// Simulated SHIP session: one `SKI:<hex>` line, answered with ACCEPTED/REJECTED.
/// The SKI must match the provisioned pairing dataset — an unpaired box accepts nobody.
async fn ship_session(
    state: BoxState,
    mut tls: tokio_rustls::server::TlsStream<tokio::net::TcpStream>,
) {
    use tokio::io::{AsyncReadExt, AsyncWriteExt};
    let mut buf = vec![0u8; 512];
    let n = match tokio::time::timeout(std::time::Duration::from_secs(10), tls.read(&mut buf)).await
    {
        Ok(Ok(n)) if n > 0 => n,
        _ => return,
    };
    let line = String::from_utf8_lossy(&buf[..n]);
    let claimed = line.trim().strip_prefix("SKI:").unwrap_or("").to_string();

    let provisioned = state
        .inner
        .lock()
        .unwrap()
        .pairing
        .as_ref()
        .map(|p| p.ski.clone());
    let accepted = provisioned.as_deref() == Some(claimed.as_str()) && !claimed.is_empty();

    if accepted {
        tracing::info!(ski = %claimed, "SHIP handshake accepted — closed loop proven");
    } else {
        tracing::warn!(ski = %claimed, "SHIP handshake REJECTED (SKI not provisioned)");
    }
    state.inner.lock().unwrap().ship = Some(ShipResult {
        ski: claimed,
        accepted,
    });

    let _ = tls
        .write_all(if accepted {
            b"ACCEPTED\n"
        } else {
            b"REJECTED\n"
        })
        .await;
    let _ = tls.shutdown().await;
}

fn env_or(name: &str, default: &str) -> String {
    std::env::var(name).unwrap_or_else(|_| default.to_string())
}

#[allow(dead_code)]
fn install_crypto_provider() {
    // deterministic crypto provider (workspace pins rustls/ring; reqwest may also pull aws-lc-rs)
    let _ = rustls::crypto::ring::default_provider().install_default();
}

fn main() -> anyhow::Result<()> {
    install_crypto_provider();
    tracing_subscriber::fmt()
        .with_env_filter(
            tracing_subscriber::EnvFilter::try_from_default_env()
                .unwrap_or_else(|_| "info,rustls=warn,tokio_rustls=warn".into()),
        )
        .init();

    let serial = env_or("BOX_SERIAL", "SNTEST0001");
    let pen = env_or("BOX_PEN", "51593");
    // box ship_id = full VPID (real-world: hems_eebus_stb_ship_id = i:<PEN>_u:<serial>)
    let ship_id = env_or("BOX_SHIP_ID", &format!("i:{pen}_u:{serial}"));
    let ski = env_or("BOX_SKI", "aabbccdd00112233445566778899aabbccddeeff");
    let state = BoxState {
        token: env_or("BOX_ADMIN_TOKEN", "box-admin-token"),
        serial: serial.clone(),
        inner: Arc::new(Mutex::new(BoxInner {
            box_ship_id: ship_id,
            box_ski: ski,
            pairing: None,
            ship: None,
        })),
    };

    let rt = tokio::runtime::Builder::new_multi_thread()
        .enable_all()
        .build()?;
    rt.block_on(async move {
        // no mDNS here: the reference HEMS has already reported the Steuerbox data cloud-side
        let ship_addr: std::net::SocketAddr =
            format!("0.0.0.0:{}", env_or("BOX_SHIP_PORT", "19543")).parse()?;
        let ship_state = state.clone();
        tokio::spawn(async move {
            if let Err(e) = ship_listener(ship_state, ship_addr).await {
                tracing::error!(error = %e, "SHIP listener died");
            }
        });

        let admin_bind: std::net::SocketAddr =
            format!("0.0.0.0:{}", env_or("BOX_ADMIN_PORT", "18642")).parse()?;
        let app = Router::new()
            .route("/admin/pairing/{serial}", post(admin_pairing))
            .route("/admin/status", get(admin_status))
            .with_state(state);
        let listener = tokio::net::TcpListener::bind(admin_bind).await?;
        tracing::info!(%admin_bind, "box-mock admin receiver up");
        tokio::select! {
            result = async { axum::serve(listener, app).await } => { result?; },
            _ = steuerbox_protocol::shutdown_signal() => {},
        }
        Ok(())
    })
}
