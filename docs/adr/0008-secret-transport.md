# ADR-0008: SHIP secret transport — mTLS provisioning payload in, admin channel out

- **Status:** Accepted (amended 2026-09-25: the SPSEC security code rides in the
  provisioning payload's `qr_info`; ADR-0017)
- **Context:** The FNN credential set contains a secret (the SHIP security code,
  `SPSEC` in the EEBUS QR content). The secret must reach the MSB once — as part of
  the QR-Info-Datensatz — and then reach the Steuerbox. Logging it anywhere would leak
  it beyond its transport.
- **Decision:** The provisioning payload carries identity data + the QR-Info-Datensatz
  INCLUDING the SPSEC security code (ADR-0017). **mTLS is the secret's transport** —
  it MUST never appear in logs, traces, error bodies or operator alarms, and the MSB
  forwards it ONLY via its existing box admin channel (Steuerbox admin / TAF on
  SMGWv2). Provisioning bodies are never logged; metadata only
  (`serial, dlms_manufacturer_id, cert_fingerprint, status, ts`). No SHIP secret in
  logs, documentation examples beyond the masked normative schema example, or audit
  output — ever.
- **Consequences:** Normative scenarios `NetworkBoundaries` + secret-freedom checks in
  conformance (no secret outside the mTLS payload); the QR channel stays RSA-encrypted
  end-to-end from the printed QR to the EMS backend decryptor.