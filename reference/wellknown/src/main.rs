//! Tiny TLS server publishing `/.well-known/steuerbox-connect/v1`.
//!
//! Document fields (see spec/wellknown.schema.json): `iana_pen` (IANA PEN of the
//! manufacturer, fleet binding ADR-0018), `client_ca`
//! (`scope: steuerbox-connect-api`, PEM), `contact`.
//! Server-TLS only (sim-LE chain); no mTLS on this endpoint.

use std::path::PathBuf;
use std::sync::Arc;

use axum::extract::State;
use axum::http::StatusCode;
use axum::response::IntoResponse;
use axum::Json;
use base64::Engine as _;
use rustls_pki_types::pem::PemObject;
use rustls_pki_types::{CertificateDer, PrivateKeyDer};
use serde_json::{json, Value};

struct WkState {
    doc: Value,
}

async fn wellknown_handler(State(state): State<Arc<WkState>>) -> impl IntoResponse {
    (StatusCode::OK, Json(state.doc.clone()))
}

/// Serve the well-known document over server-TLS. Blocks forever.
async fn serve(
    bind: std::net::SocketAddr,
    cert_path: PathBuf,
    key_path: PathBuf,
    doc: Value,
) -> anyhow::Result<()> {
    let certs = CertificateDer::pem_file_iter(&cert_path)?.collect::<Result<Vec<_>, _>>()?;
    let key = PrivateKeyDer::from_pem_file(&key_path)?;
    let config = Arc::new(
        rustls::ServerConfig::builder()
            .with_no_client_auth()
            .with_single_cert(certs, key)?,
    );

    struct Svc {
        state: Arc<WkState>,
    }
    impl hyper::service::Service<hyper::Request<hyper::body::Incoming>> for Svc {
        type Response = axum::response::Response;
        type Error = std::convert::Infallible;
        type Future = std::pin::Pin<
            Box<dyn std::future::Future<Output = Result<Self::Response, Self::Error>> + Send>,
        >;
        fn call(&self, req: hyper::Request<hyper::body::Incoming>) -> Self::Future {
            use tower::ServiceExt;
            let app = axum::Router::new()
                .route(
                    "/.well-known/steuerbox-connect/v1",
                    axum::routing::get(wellknown_handler),
                )
                .with_state(self.state.clone());
            Box::pin(async move { app.oneshot(req).await })
        }
    }

    let listener = tokio::net::TcpListener::bind(bind).await?;
    let state = Arc::new(WkState { doc });
    tracing::info!(%bind, "wellknown listening (server-TLS)");
    loop {
        let (tcp, _) = listener.accept().await?;
        let config = config.clone();
        let svc = Svc {
            state: state.clone(),
        };
        tokio::spawn(async move {
            if let Ok(tls) = tokio_rustls::TlsAcceptor::from(config).accept(tcp).await {
                let _ = hyper::server::conn::http1::Builder::new()
                    .serve_connection(hyper_util::rt::TokioIo::new(tls), svc)
                    .await;
            }
        });
    }
}

fn env_or(name: &str, default: &str) -> String {
    std::env::var(name).unwrap_or_else(|_| default.to_string())
}

/// Build the `client_ca` section per spec/wellknown.schema.json: `pem` is
/// base64-encoded (contentEncoding: base64) and the rotation grace window (ADR-0003)
/// is declared via not_before / not_after taken from the certificate's validity.
fn client_ca_section(client_ca_pem: &str) -> Value {
    let b64 = base64::engine::general_purpose::STANDARD.encode(client_ca_pem.trim());
    let (not_before, not_after) = cert_validity(client_ca_pem);
    let mut section = json!({
        "pem": b64,
        "scope": "steuerbox-connect-api",
    });
    let obj = section.as_object_mut().expect("section is an object");
    if let Some(nb) = not_before {
        obj.insert("not_before".into(), json!(nb));
    }
    if let Some(na) = not_after {
        obj.insert("not_after".into(), json!(na));
    }
    section
}

/// RFC 3339 validity window of the first certificate in the PEM, if parseable.
fn cert_validity(pem: &str) -> (Option<String>, Option<String>) {
    use x509_parser::prelude::*;
    let parsed = match parse_x509_pem(pem.as_bytes()) {
        Ok(parsed) => parsed,
        Err(e) => {
            tracing::warn!(error = %e, "cannot parse client CA — publishing without validity window");
            return (None, None);
        }
    };
    let cert = match parsed.1.parse_x509() {
        Ok(c) => c,
        Err(e) => {
            tracing::warn!(error = %e, "cannot parse client CA — publishing without validity window");
            return (None, None);
        }
    };
    let v = cert.validity();
    let rfc3339 = |ts: i64| {
        ::time::OffsetDateTime::from_unix_timestamp(ts)
            .ok()
            .and_then(|d| {
                d.format(&::time::format_description::well_known::Rfc3339)
                    .ok()
            })
    };
    (
        rfc3339(v.not_before.timestamp()),
        rfc3339(v.not_after.timestamp()),
    )
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

    let bind: std::net::SocketAddr = env_or("WK_BIND", "127.0.0.1:19443").parse()?;
    let cert_path = env_or("WK_TLS_CERT", "test/ca/out/wellknown-chain.pem").into();
    let key_path = env_or("WK_TLS_KEY", "test/ca/out/wellknown-key.pem").into();
    let iana_pen = env_or("WK_IANA_PEN", "12345");
    anyhow::ensure!(
        !iana_pen.is_empty() && iana_pen.bytes().all(|b| b.is_ascii_digit()),
        "WK_IANA_PEN must be the manufacturer's IANA PEN (digits only)"
    );
    let client_ca_pem =
        std::fs::read_to_string(env_or("WK_CLIENT_CA_PEM", "test/ca/out/client-ca-cert.pem"))?;
    let doc = json!({
        "iana_pen": iana_pen,
        "client_ca": client_ca_section(&client_ca_pem),
        "contact": env_or("WK_CONTACT", "ops@example-ems.example"),
    });

    let rt = tokio::runtime::Builder::new_multi_thread()
        .enable_all()
        .build()?;
    rt.block_on(async move {
        tokio::select! {
            result = serve(bind, cert_path, key_path, doc) => result,
            _ = steuerbox_protocol::shutdown_signal() => Ok(()),
        }
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    // The published client_ca section must satisfy spec/wellknown.schema.json:
    // `pem` is base64-encoded (contentEncoding: base64) and the rotation grace window
    // (ADR-0003) is declared via not_before / not_after taken from the certificate.

    const TEST_CA_PEM: &str = include_str!("../../../spec/examples/test-client-ca.pem");

    #[test]
    fn pem_is_published_base64_encoded() {
        let section = client_ca_section(TEST_CA_PEM);
        let b64 = section["pem"].as_str().expect("pem must be a string");
        assert!(!b64.contains("BEGIN CERTIFICATE"));
        let decoded = base64::Engine::decode(&base64::engine::general_purpose::STANDARD, b64)
            .expect("pem must be valid base64");
        let decoded = String::from_utf8(decoded).unwrap();
        assert!(decoded.starts_with("-----BEGIN CERTIFICATE-----"));
    }

    #[test]
    fn scope_is_published() {
        let section = client_ca_section(TEST_CA_PEM);
        assert_eq!(section["scope"], "steuerbox-connect-api");
    }

    #[test]
    fn grace_window_is_published_from_cert_validity() {
        let section = client_ca_section(TEST_CA_PEM);
        let nb = section["not_before"].as_str().expect("not_before");
        let na = section["not_after"].as_str().expect("not_after");
        assert!(nb.starts_with("2026-09-25"), "not_before = {nb}");
        assert!(na.starts_with("2026-10-25"), "not_after = {na}");
    }
}
