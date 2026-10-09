# spec/examples/ — canonical examples

Every example validates against the corresponding schema (`make spec-validate`
enforces this). Serials use the `SNTEST*` masks only (AGENTS.md §6 — no real
serials in fixtures). Error codes are machine-readable; the status mapping is
documented in each OpenAPI file's `ErrorResponse` schema.

| File | Validates against | Used by |
|---|---|---|
| `provisioning-request.json` | `provisioning.yaml` + `pairing_dataset.schema.json` | provisioning requestBody example |
| `pairing-dataset.json` | `pairing_dataset.schema.json` | reference for manufacturers |
| `wellknown-v1.json` | `wellknown.schema.json` | reference for manufacturers (base64 `client_ca.pem`, grace window `not_before`/`not_after`) |

Discovery is status-code based: 200 found / 404 not found (empty bodies) plus 429;
provisioning 202 / 400 / 404 / 409 / 422 / 429.