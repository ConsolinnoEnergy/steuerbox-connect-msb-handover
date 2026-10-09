# ADR-0004: Tolerant matching (longest portfolio prefix + `-` delimiter) with anti-enumeration

- **Status:** Accepted — the "longest portfolio prefix" clause is **superseded** by [ADR-0012](0012-prefixless-matching.md) (prefixless matching)
- **Context:** Steuerboxen announce their serial per mDNS inside the SHIP-ID
  (`i:<IANA-PEN>_u:<VPID>`); portfolio serials may carry `-` suffixes, and the query key
  is the manufacturer-agnostic id observed in the LAN.
- **Decision:** Discovery matches tolerantly (longest portfolio prefix + `-` delimiter)
  and always returns the canonical serial. A query **shorter** than a portfolio serial
  MUST answer `404` (anti-enumeration; discovery is status-code based, ADR-0005). Provisioning writes only on the canonical
  serial — never on a client-derived serial. Serial derivation follows Regel S
  (split at the first `-`; canonical serials match `^[A-Za-z0-9]{4,32}$`, no hyphen).
- **Consequences:** Portfolio scans are impractical; client-side derivation bugs cannot
  create writes. Parser correctness is guaranteed by golden records, never by network e2e.