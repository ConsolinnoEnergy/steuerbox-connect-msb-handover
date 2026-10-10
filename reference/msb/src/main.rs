//! Rust reference MSB binary — thin wrapper around `msb-core`.
//!
//! The reference MSB exists so that e2e/compose/conformance have a working peer;
//! real MSBs embed `msb-core` (library) into their backend with their own DB —
//! see `docs/rolle/msb.md` and ADR-0015. Environment differences live in config
//! only (AGENTS.md §4).

use std::collections::HashMap;
use std::net::SocketAddr;
use std::sync::Arc;

use msb_core::admin::{BoxAdminAdapter, DisabledBoxAdmin, HttpBoxAdmin};
use msb_core::api::MsbState;
use msb_core::store::{InMemoryStore, InventoryEntry, MsbStore};

fn env_or(name: &str, default: &str) -> String {
    std::env::var(name).unwrap_or_else(|_| default.to_string())
}

fn install_crypto_provider() {
    // deterministic crypto provider (workspace pins rustls/ring; reqwest may also pull aws-lc-rs)
    let _ = rustls::crypto::ring::default_provider().install_default();
}

fn main() -> anyhow::Result<()> {
    install_crypto_provider();
    tracing_subscriber::fmt()
        .with_env_filter(
            tracing_subscriber::EnvFilter::try_from_default_env()
                .unwrap_or_else(|_| "info,hyper=warn,tokio_rustls=warn,rustls=warn".into()),
        )
        .init();

    let api_bind: SocketAddr = env_or("MSB_BIND", "127.0.0.1:18443").parse()?;
    let admin_bind: SocketAddr = env_or("MSB_ADMIN_BIND", "127.0.0.1:18444").parse()?;
    let inventory: HashMap<String, InventoryEntry> = match std::env::var("MSB_INVENTORY_FILE") {
        // k8s deployment: portfolio mounted as a secret file (real serials
        // never live in git or manifests — injected via CI variable)
        Ok(path) => serde_json::from_str(&std::fs::read_to_string(&path)?)?,
        Err(_) => serde_json::from_str(&env_or("MSB_INVENTORY", "{}")).unwrap_or_default(),
    };

    let store: Arc<dyn MsbStore> = Arc::new(InMemoryStore::new(inventory));
    let box_admin: Arc<dyn BoxAdminAdapter> = match std::env::var("MSB_BOX_ADMIN_URL").ok() {
        Some(url) => Arc::new(HttpBoxAdmin {
            base_url: url,
            token: env_or("MSB_BOX_ADMIN_TOKEN", "box-admin-token"),
            client: reqwest::Client::new(),
        }),
        None => Arc::new(DisabledBoxAdmin),
    };

    let state = Arc::new(
        MsbState::new(
            store,
            env_or("MSB_MSB_ID", "msb-reference"),
            env_or("MSB_RATE_LIMIT", "1000").parse()?,
            box_admin,
            env_or("MSB_ADMIN_TOKEN", "admin-test-token"),
        )
        .with_iana_pen(env_or("MSB_IANA_PEN", "12345"))
        .with_tls(
            env_or("MSB_TLS_CERT", "test/ca/out/msb-chain.pem").into(),
            env_or("MSB_TLS_KEY", "test/ca/out/msb-key.pem").into(),
            env_or("MSB_CLIENT_CA_DIR", "test/ca/out/pins").into(),
        ),
    );

    let rt = tokio::runtime::Builder::new_multi_thread()
        .enable_all()
        .build()?;
    rt.block_on(async move { msb_core::api::serve(state, api_bind, admin_bind).await })
}
