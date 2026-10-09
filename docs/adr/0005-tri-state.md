# ADR-0005: Tri-state semantics — transport failure is `unknown`, never `found:false`

- **Status:** Accepted
- **Context:** Distinguishing "not in portfolio" from "MSB unreachable" decides whether
  zero-touch pairing proceeds. A wrong `not found` silently blocks a customer; a wrong
  `found` cannot happen structurally.
- **Decision:** Both sides implement tri-state, but on **different layers**:
  `found`, `not found`, `unknown`. Transport/TLS/availability failures are
  `unknown`. The client refuses provisioning on `unknown`. Never map errors to
  `not found` on either side.
- **Clarification (2026-09-27):** `unknown` is a **client-side state only** — it is
  derived from transport failure, TLS rejection or any non-200/404 answer and is
  **never transmitted**. Discovery is **status-code based** (`spec/discovery.yaml`
  v0.3.0): `200` (empty body) = found, `404` (empty body) = not found, `429` +
  Retry-After = rate limited. The canonical serial is NOT transmitted — the client
  derives it deterministically as the Regel-S base of its own query (ADR-0012).
  "Server-side tri-state" means: the server never answers HTTP-error for a decidable
  query (found/not-found are 200/404); transport failures map to `unknown` client-side.
- **Consequences:** Normative scenario `TriState_Unknown`. Error taxonomy in the spec
  separates HTTP-decidable answers from transport failures.