# ADR-0012: Prefixless tolerant matching (exact Regel-S base equality)

- **Status:** Accepted (supersedes the longest-prefix decision of ADR-0004)
- **Context:** ADR-0004 prescribed "longest portfolio prefix + `-` delimiter" matching.
  Two problems surfaced: (1) prefix matching structurally defeats the anti-enumeration
  rule it co-defines — a prefix query that matches a portfolio entry would have to
  return `200`, yet ADR-0004 itself mandates `404` for shorter queries;
  (2) the reference implementation (`src/msb/src/matcher.rs`) and the spec
  (`spec/discovery.yaml` v0.2.0) never implemented prefix matching. Tolerant matching
  only ever concerned the *form* of the query, never its length.
- **Decision:** Discovery matching is **prefixless**: both the query and the portfolio
  serial are reduced to their Regel-S base (split at the first `-`; no hyphen → whole
  string) and compared for **exact equality** of the base. Tolerated query forms:
  the canonical serial, a `-suffix` variant of the same serial, and the full VPID form
  `i:<IANA-PEN>_u:<VPID>`. A query whose base is shorter than a portfolio serial never
  matches (anti-enumeration). The answer always carries the canonical (hyphen-free)
  serial, which remains the only valid provisioning write target. Provisioning writes
  only on the canonical serial — never on a client-derived serial.
- **Consequences:** Portfolio scans are impractical (every probe costs a full-hit event);
  client-side derivation bugs cannot create writes; ADR-0004's "longest portfolio
  prefix" clause is void. Matching semantics change tests: unit tests against golden
  records and the e2e scenario `AntiEnumeration` are the normative proof.