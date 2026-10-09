# ADR-0014: Accepted residual risk — `rsa` crate (RUSTSEC-2023-0071, Marvin Attack)

- Status: accepted (dated 2026-09-25)
- Deciders: eebus-pairing maintainers
- Related: AGENTS.md §5 (audit gates), `deny.toml`, RUSTSEC-2023-0071,
  RustCrypto/RSA#626

## Context

`cargo deny check advisories` reports **RUSTSEC-2023-0071**: the RustCrypto `rsa`
crate has a non-constant-time RSA implementation ("Marvin Attack"); private-key
material may leak via network-observable timing. As of 2026-09 the advisory
declares `patched = []` — **no fixed release exists** (stable 0.9.10 and rc
0.10.x are both still affected; tracking issue RustCrypto/RSA#626).

The advisory is unavoidable for this workspace because `rsa 0.9` is a direct
dependency:

- `src/ems-backend/src/audit.rs` — RSA-OAEP (SHA-256) **decryption** of audit
  relay payloads with a locally provisioned private key, plus a unit test doing
  a local encrypt/decrypt roundtrip.
- `src/hems-mock/src/main.rs` — public-key handling for the same relay format.

Removing `rsa` would mean dropping or redesigning the audit-relay feature, which
is out of scope for the PR that introduced the dependency (see AGENTS.md §5:
"A new vuln gate MUST be added in the same PR"; unfixable findings need an ADR
plus a dated ignore entry).

## Why the risk is accepted

1. **No patch exists.** There is no version we could upgrade to; "fix" is not an
   available action today.
2. **Reachability is limited.** The Marvin Attack requires an attacker able to
   observe many decryption timings. In this workspace RSA is used exclusively
   for OAEP decryption of audit payloads on the EMS backend with a locally held
   key — there is no RSA key exchange, no RSA TLS handshake, and no RSA
   signature verification on any network-facing path. All mTLS/TLS cryptography
   (MSB API, well-known, client mTLS, SHIP-TLS stubs) is `rustls`/`p256`
   (ECDSA P-256), per the RustCrypto-only rule (AGENTS.md §6).
3. **Trust boundary.** Audit-relay payloads are submitted by the manufacturer's
   own backend (authenticated channel), not by anonymous clients, which sharply
   limits an attacker's ability to collect adaptive timing samples.

## Consequences

- `deny.toml` carries a dated ignore entry for `RUSTSEC-2023-0071` with this
  ADR as justification; the CI `audit` gate stays red-free.
- **Re-review trigger:** a patched `rsa` release (RustCrypto/RSA#626/#702) or
  any change that exposes an RSA decrypt path to untrusted callers. Either
  invalidates this ADR and requires an upgrade or a redesign.
- Mid-term direction: migrate the audit-relay cryptography to a hybrid/ECIES
  scheme on P-256 so the `rsa` dependency can be dropped entirely; tracked as a
  backlog item. New crypto dependencies require an ADR per AGENTS.md §6.