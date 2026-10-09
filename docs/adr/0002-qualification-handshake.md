# ADR-0002: Bilateral qualification handshake

- **Status:** Accepted (amended 2026-09-25: EEBUS Cert-ID anchor replaced by the
  DLMS manufacturer flag id, see ADR-0016)
- **Context:** ~4 MSB service providers × ~6 EMS manufacturers make a central registry
  unnecessary; bilateral onboarding must still be unambiguous and auditable.
- **Decision:** One-time bilateral qualification per pair: the manufacturer shares its
  well-known URI, the MSB shares API base URL, the two endpoints and quotas. The anchor
  is the manufacturer's **DLMS flag id** (publicly registered, ADR-0016 — e.g. `CEG`),
  combined with out-of-band identity verification (personally/verified channel). No
  central registry, no bilateral credential stores.
- **Consequences:** Coupling scales as O(n·m) with URL dedupe (~4–6 queries). Rotation
  and endpoint changes are covered by the Ops-Contract (notification duty), not by an
  API endpoint. Trust remains bound to the pinned client CA, not to the flag id — the
  id identifies, it does not authorize.
- **Note:** The original anchor (EEBUS Cert-ID) is removed: the EEBUS certification
  scheme does not provide Cert-IDs yet. Once it does, a new ADR can reintroduce it as
  an additional anchor.