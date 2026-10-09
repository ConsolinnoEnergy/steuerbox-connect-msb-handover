# ADR-0013: Well-known wire format — base64 `client_ca.pem` and validity window

- **Status:** Accepted (amends ADR-0003)
- **Context:** ADR-0003 states "grace windows and rotation are declared in the document"
  without fixing a wire format. `spec/wellknown.schema.json` requires `client_ca.pem`
  as base64-encoded content (`contentEncoding: base64`) and defines
  `client_ca.not_before` / `not_after` as optional RFC 3339 timestamps. Consumers and
  publishers need one binding wire format; a silently expired pin is a rotation bug.
- **Decision:**
  1. `client_ca.pem` is transported **base64-encoded**; the plain PEM exists only
     after decoding (pin store, trust anchors). Raw PEM on the wire is invalid.
  2. The publisher derives `not_before` / `not_after` (RFC 3339, UTC) from the
     certificate validity and publishes them. The fetcher decodes the PEM and
     **rejects** documents whose window does not cover the current time
     (not yet valid / expired) — fetches outside the window fail loudly, never
     silently pin an expired CA.
  3. The document models a **single** CA window. "Old and new CA parallel valid"
     (rotation grace) is NOT expressed in the document; it is implemented by the MSB
     pin store accepting multiple pinned CAs during rotation (file-based, watched
     per connection). The document declares the current CA's window only.
  4. No `jwks_uri` and no `eebus` section in the document. Device authentication at the
     EMS backend is the manufacturer's concern (e.g. JWT) and outside this repo — nothing
     about it is published or normative here. The document only serves backend-to-backend
     trust (pinned dedicated client sub-CA). (Amendment 2026-09-25: `jwks_uri` removed —
     no JWS/JWKS verification happens in this repo. `eebus.cert_ids` removed — the EEBUS
     scheme provides no Cert-IDs yet, see ADR-0002/-0016.)
- **Consequences:** Rotations are detectable and enforceable (expired window → hard
  failure). The e2e rotation path (`rotate-demo`, `ClientCARotation`) exercises the
  pin-store grace; document-driven rotation (well-known update → re-pin) is the
  manufacturer-side counterpart and MUST honor `allow_rotation` semantics
  (see `src/ems-backend/src/wellknown.rs`).