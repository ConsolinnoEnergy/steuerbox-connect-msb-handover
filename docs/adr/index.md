# Architekturentscheidungen (ADRs)

ADRs sind **bindende Entscheidungen** (Sprache: Englisch, gemäß Language-Policy).
Bevor Sie Trust, TLS, Matching oder Fehler-Semantik anfassen: ADR lesen bzw. schreiben.

Template: **Nummer · Status · Context · Decision · Consequences.**

## Index

| Nummer | Thema | Status |
|---|---|---|
| [0001](0001-two-ca-model.md) | Two-CA model — separate server-TLS and client-mTLS trust domains | Accepted |
| [0002](0002-qualification-handshake.md) | Bilateral qualification handshake (IANA PEN anchor, ADR-0018; EEBUS Cert-ID removed — not yet available) | Accepted (amended 2026-10-10) |
| [0003](0003-wellknown-publishes-client-ca.md) | Well-known document publishes the dedicated client CA | Accepted |
| [0004](0004-tolerant-matching-anti-enumeration.md) | Tolerant matching + anti-enumeration (Regel S) | Accepted, partially superseded by 0012 |
| [0005](0005-tri-state.md) | Tri-state semantics — transport failure is `unknown` | Accepted (clarified 2026-09-25) |
| [0006](0006-idempotency-write-once.md) | Idempotency on `(serial, ship_id)`; write-once | Accepted |
| [0007](0007-box-evidence.md) | `box_evidence` verification, `422 box_mismatch` + alarm | Accepted |
| [0008](0008-secret-transport.md) | No SHIP secret via the API — admin channel only | Accepted |
| [0009](0009-mtls-in-app-termination.md) | mTLS terminates inside the application | Accepted |
| [0010](0010-rate-limits-contract.md) | Rate limits are contract (`429` + truthful `Retry-After`) | Accepted |
| [0011](0011-mdns-policy-closed-loop.md) | mDNS golden-record policy; closed loop as success proof | Accepted |
| [0012](0012-prefixless-matching.md) | Prefixless tolerant matching — exact Regel-S base equality | Accepted (supersedes ADR-0004 prefix clause) |
| [0013](0013-wellknown-wire-format.md) | Well-known wire format — base64 `client_ca.pem`, validity window | Accepted (amends ADR-0003) |
| [0014](0014-rsa-marvin-accepted-risk.md) | Accepted residual risk: `rsa` crate (RUSTSEC-2023-0071, Marvin Attack) | Accepted (2026-09-25) |
| [0015](0015-msb-core-library.md) | `msb-core` embeddable library + `MsbStore` integration seam for MSB backends | Accepted (2026-09-25) |
| [0016](0016-manufacturer-id.md) | Publicly registered manufacturer identifier instead of self-chosen `manufacturer_id` | Superseded by ADR-0018 (2026-10-10) |
| [0017](0017-slim-provisioning-payload.md) | Slim provisioning payload — `pairing_dataset {ship_id, ski, qr_info}` only | Accepted |
| [0018](0018-iana-pen-fleet-binding.md) | `iana_pen` in the well-known document replaces the registered manufacturer identifier; fleet binding on provisioning (`422 fleet_mismatch`) | Accepted (2026-10-10, supersedes ADR-0016) |

## Themen, die einen neuen ADR erfordern

- Änderungen an `spec/` (OpenAPI, Schemas)
- TLS-/Trust-Änderungen (Zwei-CA-Modell, Pinning, Rotation)
- Matching-Semantik (Regel S, tolerantes Matching, Anti-Enumeration)
- Fehler-Semantik (Tri-State, `422 box_mismatch`, Rate Limits)
- Neue Krypto-Dependencies (RustCrypto-only-Regel)
- Löschen oder Abschwächen eines e2e-Szenarios
- Unfixbare Vulnerability-Findings (datierte Ignore-Einträge in `deny.toml`)