# spec/ — normative

OpenAPI 3.1 (`discovery.yaml`, `provisioning.yaml`), JSON Schemas
(`wellknown.schema.json`, `pairing_dataset.schema.json`), examples (`examples/`).

If code disagrees with this directory, the code is wrong (AGENTS.md §3).

Validate: `make spec-validate` — parses both OpenAPI files, resolves every external
`$ref`, and validates `examples/` against the JSON Schemas.

## Error codes (machine-readable, `ErrorResponse`)

All error bodies have the shape `{ "error": code, "message": "…" }`.

| Status | Code | Meaning |
|---|---|---|
| 400 | `not_canonical` | path serial is not the canonical serial from discovery |
| 404 | `not_found` | serial not in portfolio / killed (no existence leak) |
| 409 | `write_once_conflict` | serial already paired to a different ship_id |
| 422 | `box_mismatch` | supplied box_evidence does not match inventory (partial evidence counts as mismatch) |
| 422 | `invalid_pairing_dataset` | pairing_dataset incomplete/invalid (HEMS serial outside `^[A-Za-z0-9_-]{4,64}$`) |
| 422 | `invalid_body` | request body is not a valid provisioning request |
| 429 | `rate_limited` | quota exceeded; `Retry-After` header mandatory on both endpoints |

## Normative decisions captured here

- **Tri-state placement:** discovery is status-code based — the server answers
  `200` found / `404` not found (empty bodies); `unknown` is derived client-side
  (transport failure, any other status) — it is never on the wire (ADR-0005,
  spec/discovery.yaml).
- **Kill switch:** killed serials read `404` (discovery) / `404 not_found`
  (provisioning) without leaking that the serial ever existed.
- **Rate limits are contract:** 429 + `Retry-After` (seconds, numeric) on BOTH
  endpoints; a 429 without the header is a server contract violation — clients
  MUST NOT retry with a fabricated delay.
- **box_evidence unverifiable:** supplied evidence without an inventory entry is
  accepted, but MUST raise an operator alarm — never a silent ignore.
- **Well-known:** `client_ca.pem` is base64-encoded (`contentEncoding: base64`);
  the rotation grace window is declared via `client_ca.not_before`/`not_after`
  (RFC 3339, from the certificate validity, ADR-0003).