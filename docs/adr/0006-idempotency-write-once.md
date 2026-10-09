# ADR-0006: Provisioning is idempotent on `(serial, ship_id)`; write-once on serial↔MSB

- **Status:** Accepted
- **Context:** Retries and fan-out must be safe; serial↔MSB binding is a physical
  invariant (MSB switch = physical box swap).
- **Decision:** `POST /v1/serials/{serial}/provisioning` is idempotent on
  `(serial, ship_id)`: repeated POSTs yield `202`/`202`/`409` semantics. Serial↔MSB
  never changes; there are no transfer or ownership endpoints.
- **Consequences:** Normative scenario `IdempotentPost`. No transfer/ownership
  endpoints exist; "Bewusst nicht gelöst" documents the physical invariant.