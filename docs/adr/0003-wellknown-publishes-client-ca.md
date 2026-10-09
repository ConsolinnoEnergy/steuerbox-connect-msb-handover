# ADR-0003: Well-known document publishes the dedicated client CA

- **Status:** Accepted
- **Context:** The MSB must validate mTLS client certificates issued by each EMS
  manufacturer without manual certificate distribution.
- **Decision:** The EMS manufacturer publishes `/.well-known/steuerbox-connect/v1`
  containing `dlms_manufacturer_id` (DLMS flag id, ADR-0016), `client_ca` (public part of a
  **dedicated** sub-CA, `scope: steuerbox-connect-api`) and `contact`. The DLMS flag id (ADR-0016)
  are exchanged bilaterally during the qualification handshake (ADR-0002), not
  published in the well-known document.
  The MSB fetches the document, pins the CA (TOFU + TTL) and validates every client
  certificate against it. Grace windows and rotation are declared in the document.
- **Amendment (2026-09-25):** The wire format is now binding via
  [ADR-0013](0013-wellknown-wire-format.md): `client_ca.pem` is base64-encoded,
  `not_before`/`not_after` are RFC 3339 and enforced by the fetcher; parallel CA
  validity (grace) lives in the MSB pin store, not in the document. `jwks_uri` was
  removed from the document (mTLS-based backend verification; see ADR-0013). In the
  reference implementation the MSB reads its pinned CAs from a local, watched pin directory
  (`MSB_CLIENT_CA_DIR`); the **well-known fetch is performed by the EMS backend**
  (client side) and is used for pin/rotation detection on that side.
- **Consequences:** Autonomous rotation with zero-touch re-pinning. TOFU residual risk
  is mitigated by the out-of-band qualification handshake (see threat-model.md, T1/T3).