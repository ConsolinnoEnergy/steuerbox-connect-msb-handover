//! `msb-core` — embeddable reference implementation of the MSB side of the
//! steuerbox-connect protocol (see `spec/`, normative OpenAPI).
//!
//! An MSB embeds the protocol logic into its own backend by:
//!
//! 1. adding `msb-core` as a dependency,
//! 2. implementing [`store::MsbStore`] against its real portfolio/inventory DB
//!    (four methods — matching semantics come from [`matcher`]),
//! 3. building the mTLS API with [`api::api_router`] + [`api::serve_tls`]
//!    (mTLS terminates inside the application, ADR-0009),
//! 4. optionally implementing [`admin::BoxAdminAdapter`] for its box-admin path.
//!
//! [`store::InMemoryStore`] is the reference dummy (used by the `msb` binary,
//! the Compose stack and e2e); it can also serve as a starting point.
//!
//! Hard rules enforced here (AGENTS.md §6): mTLS-only, tri-state, write-once,
//! box_evidence mismatch → 422, 429 + Retry-After, no SHIP secret in payloads
//! or logs, provisioning bodies never logged.

pub mod admin;
pub mod api;
pub mod matcher;
pub mod store;
pub mod tls;

pub use store::{InMemoryStore, InventoryEntry, MsbStore, PairingRecord, ProvisionOutcome};
