# Spezifikation

Die OpenAPI-Spezifikation und JSON-Schemas unter `spec/`
sind **normativ**. Wenn Code der Spec widerspricht, ist der Code falsch.

| Artefakt | Inhalt |
|---|---|
| `spec/discovery.yaml` | `GET /v1/serials/{serial\|vpid}` — tolerant Matching, kanonische Serial, Anti-Enumeration |
| `spec/provisioning.yaml` | `POST /v1/serials/{serial}/provisioning` — Pairing-Dataset + `box_evidence` |
| `spec/pairing_dataset.schema.json` | ship_id, ski, qr_info (SHIP-Info-Datensatz inkl. SPSEC — mTLS ist der Transport) — ADR-0017 |
| `spec/wellknown.schema.json` | Well-known-Dokument des EMS-Herstellers |
| `spec/examples/` | Beispiel-Requests/-Responses |

## Endpunkte (mTLS-only)

Entwicklerfreundliche REST-API-Beschreibung mit Payloads und Responses:
[REST-API-Referenz](../api.md) — inkl. kombinierter OpenAPI-Spec als
[Download](../api/openapi.json). Die Kernregeln in Kurzform:

- **Discovery** `GET /v1/serials/{serial|vpid}`: status-code basiert — `200` = found,
  `404` = not found, leerer Body (kanonische Serial leitet der Client selbst ab:
  Regel-S-Basis, präfixlos, ADR-0012); Anti-Enumeration (kürzere Query → `404`);
  Tri-State: Transportfehler ist clientseitig `unknown`, nie `404`.
- **Provisioning** `POST /v1/serials/{serial}/provisioning`: ship_id, SKI,
  `qr_info` (SHIP-Info-Datensatz **inkl. SPSEC** — mTLS ist der Transport, nie in Logs)
  + `box_evidence`; idempotent auf `(serial, ship_id)`, `202 Accepted`;
  `422 box_mismatch` bei Mismatch (+ Operator-Alarm, nie still);
  Write nur auf kanonischer Serial.

## Matching-Regeln

**Regel S** (deterministische Serial-Derivation):

- Split am **ersten** `-`; Suffix optional; ohne Hyphen → ganzer String.
- Kanonische Serials: `^[A-Za-z0-9]{4,32}$` (keine Hyphens).
- Zwei Serials, nie vermischt: Pfad/Query = Steuerbox-Serial;
  die HEMS-Identität lebt ausschließlich in `pairing_dataset.ship_id` (VPID-Teil) —
  `pairing_dataset` trägt keine separate Serial mehr (ADR-0017).

Beispiel aus dem Feld (TXT-Record, wertloser `serial`-Key): die Serial steckt in
`id=VPID` → `i:51593_u:GTHE0100045088`; HEMS-Serials haben das Format `1U0022_CO-4206`;
der QR-Inhalt ist ein SHIP-String (`SHIP;SKI:…;ID:…;FPH256:…;ENDSHIP;`) und enthält
**nur lokale HEMS-Daten**.

## Bekannte Spec-Lücken

!!! warning "Offene Punkte vor Umsetzung als normativ"

    - **Gerätee-Token:** Die Geräte-Authentifizierung am EMS-Backend (z.B. JWT) ist
      **außerhalb dieses Repos** — hier taucht sie nicht normativ auf. Ein
      `attestation`-Relay wurde aus dem Payload entfernt (ADR-0017, 2026-09-27).

Änderungen an `spec/`, TLS, Trust, Matching oder Fehler-Semantik erfordern einen
(geprüften) ADR — s. [ADRs](../adr/index.md). Validierung: `make spec-validate`.