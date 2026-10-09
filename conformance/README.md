# conformance/ — black-box pytest suite

Runs against ANY MSB implementation without knowledge of internals. Checks:
mTLS enforced, error taxonomy, tri-state correctness, tolerant matching incl.
anti-enumeration, idempotency, 429 + Retry-After, 422 box_mismatch, secret-free
payloads, write-once.

```bash
make conformance TARGET=https://msb-staging.example
```

Structure: one pytest file per normative property; scenarios mirror `test/e2e`.
The reference MSB must always pass this suite (CI gate).