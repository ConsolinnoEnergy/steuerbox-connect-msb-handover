# conformance/ — black-box pytest suite

Runs against ANY MSB implementation without knowledge of internals. Checks:
mTLS enforced, error taxonomy, tri-state correctness, tolerant matching incl.
anti-enumeration, idempotency, 429 + Retry-After, 422 box_mismatch, secret-free
payloads, write-once.

```bash
make conformance TARGET=https://msb-staging.example
make conformance-local  # fresh local reference stack; no skipped rate-limit tests
```

Die Server-Zertifikatsprüfung ist immer aktiv. Für eine eigene Test-CA muss
`CONF_ROOT` auf das vertrauenswürdige CA-Bundle (PEM) zeigen; ohne `CONF_ROOT`
werden die Standard-Vertrauensanker von Requests verwendet. `CONF_CERT` und
`CONF_KEY` enthalten die Pfade zum mTLS-Client-Zertifikat und dessen Schlüssel.
`make conformance-local` setzt diese drei Variablen automatisch für die lokale
Test-PKI.

Structure: one pytest file per normative property; scenarios mirror `reference/e2e`.
The reference MSB must always pass this suite (CI gate).
