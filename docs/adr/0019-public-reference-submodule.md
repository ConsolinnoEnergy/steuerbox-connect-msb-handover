# ADR-0019: Public reference workspace as a GitHub submodule

- **Status:** Accepted
- **Context:** The standalone MSB handover package must reproduce the shared
  backend-to-backend flow without private services, deployment configuration or
  device data. Maintaining copied Rust components in two repositories causes drift.
- **Decision:**
  1. The GitHub repository owns `reference/`, the public protocol, mock services,
     local PKI tooling, synthetic golden records and 15 public E2E scenarios.
  2. The private consumer embeds a pinned GitHub commit as a submodule and uses
     path dependencies for shared libraries. Shared changes can originate in
     either checkout and are published through the same GitHub repository.
  3. The private inventory scenario belongs to the private consumer. All 15
     public scenarios retain their assertions; no public scenario is removed.
  4. This package reproduces the existing cloud-observation mode: TXT input
     through the normative parser and a simulated TLS/SKI closed loop. Real
     multicast discovery and a complete SHIP stack remain field-side concerns.
     This supersedes ADR-0011's claim that this backend suite uses real mDNS by
     default; parser correctness and the closed-loop requirement remain binding.
  5. Public identities and fixtures are synthetic; credentials are regenerated
     locally. PKI has separate server-TLS and client-mTLS roots.
  6. The shared E2E runner is a library used by both entry points. The private
     runner adds only its private inventory scenario. The consumer's spec,
     conformance, docs and fixtures paths resolve into the submodule.
  7. The documentation OpenAPI JSON is generated from the normative YAML/JSON
     sources, including the existing well-known endpoint. External example
     payloads use OpenAPI `externalValue`; there is no second handwritten API spec.
  8. `steuerbox-protocol` owns shared matching, VPID, canonical derivation and
     fleet rules. MSB, EMS client and HEMS parser use these functions directly.
     TLS client construction (custom roots or WebPKI) also has one implementation.
  9. Discovery accepts short anti-enumeration queries and full suffixed VPIDs;
     the canonical serial's 4–32 character bound applies to provisioning, not
     to the untrusted discovery query. Schema regression checks enforce this.
- **Consequences:** Public checks run independently and in the consumer. CI
  initializes submodules recursively. The shared commit must be pushed before
  committing the consumer's gitlink. `docs/entwicklung.md` defines both directions.
