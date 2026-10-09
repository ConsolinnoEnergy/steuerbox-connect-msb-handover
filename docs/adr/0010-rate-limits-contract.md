# ADR-0010: Rate limits are contract — `429` + `Retry-After`, clients MUST honor

- **Status:** Accepted
- **Context:** Serial discovery is the most abusable endpoint (enumeration, DoS). Quotas
  exist per the qualification handshake.
- **Decision:** Rate limits are part of the contract: the MSB answers `429` with a
  **truthful** `Retry-After`; clients MUST honor it (no speculative retries before
  `Retry-After`). Quotas are exchanged in the qualification handshake.
- **Consequences:** Normative scenario `RateLimit`; `Retry-After` correctness is tested
  (a lying `Retry-After` is a conformance failure).