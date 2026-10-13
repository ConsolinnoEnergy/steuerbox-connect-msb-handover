# ADR-0011: mDNS reliability via golden records; closed loop is the only success proof

- **Status:** Accepted; network-mode description superseded by [ADR-0019](0019-public-reference-submodule.md).
- **Context:** The e2e suite runs with real mDNS by default; flaky runners must not
  weaken the parser. LAN claims (mDNS TXT) are untrusted input.
- **Decision:** Two policies, non-negotiable:

  1. **Parser correctness is guaranteed by golden records** (`fixtures/golden/`, masked
     `SNTEST*` serials) — never weaken parser tests to make an e2e green. `E2E_DISCOVERY=stub`
     injects TXT records through the same normative parser; both modes are normative.
  2. **Closed loop:** only an accepted SHIP handshake in the LAN proves the write reached
     the physical box. `202 Accepted` proves nothing. Spoofed mDNS may waste a lookup —
     it may never become a coupling.

- **Consequences:** Every scenario in `AGENTS.md` §7 must keep proving these policies;
  deleting or weakening one requires an ADR.
