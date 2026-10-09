# ADR-0007: box_evidence — server MUST accept without it, SHOULD verify, mismatch → 422 + alarm

- **Status:** Accepted
- **Context:** The client sends in-LAN observed Steuerbox ship_id + ski with provisioning.
  Accepting them unconditionally would let spoofed mDNS claims become couplings.
- **Decision:** The server MUST accept provisioning **without** `box_evidence` (SHOULD by
  the client). If present, the server SHOULD verify against inventory. A mismatch
  answers `422 box_mismatch` and raises an operator alarm — never a silent ignore.
- **Consequences:** Normative scenario `BoxEvidenceMismatch`. Mismatch is a security
  event, not a validation detail; silence would hide spoofing.