# ADR-0017: Slim provisioning payload — ship_id, ski, qr_info (no separate serial/fph/base_uri)

- **Status:** Accepted
- **Datum:** 2026-09-25
- **Kontext:** Provisioning payload (amends ADR-0006 context; closes the QR-Info-Datensatz
  spec gap documented in `docs/spec/index.md`)

## Kontext

Das `pairing_dataset` trug `serial`, `fph` und `base_uri` als eigene Felder. Alle drei
sind redundant:

- **HEMS-Serial:** steckt bereits in der `ship_id` (`i:<IANA-PEN>_u:<VPID>`, VPID-Teil =
  Serial) — ein separates Feld lädt zum Mischen der zwei Serials ein (hard rule).
- **FPH:** steckt bereits im QR-Inhalt (`SHIP;…;FPH256:<hex>;ENDSHIP;`).
- **base_uri:** die EMS-Koordinaten ergeben sich aus der Qualifikation (API-Basis-URL ist
  bilateral getauscht), nicht aus der Provisionierung.

Gleichzeitig war der QR-Info-Datensatz nur prosaisch beschrieben, aber kein Schema-Feld.

## Entscheidung

`pairing_dataset` = **`ship_id`, `ski`, `qr_info`** (amended 2026-09-27: the optional
`attestation` field was removed — device authentication is entirely the EMS
manufacturer's concern and never crosses the MSB API).
`serial`, `fph` und `base_uri` entfallen aus dem Schema und allen Referenz-Implementierungen.
`qr_info` ist der SHIP-Info-Datensatz aus dem EEBUS-QR-Inhalt, normativ mit Pattern
`^SHIP;.+ENDSHIP;$`. Idempotenz bleibt auf `(serial, ship_id)` — `serial` ist die
kanonische Steuerbox-Serial im Pfad, nicht mehr im Body.

## Konsequenzen

- Spec: `pairing_dataset.schema.json` v-Änderung, `provisioning.yaml` 0.3.0; die 422
  `invalid_pairing_dataset`-Beispiele validieren nun gegen die `qr_info`-Form.
- Referenz: `msb-core` `PairingRecord`/Validierung, `box-mock` Admin-Receiver,
  `ems-backend` (baut `qr_info` aus dem dekodierten QR-Inhalt), `hems-mock` Evidence,
  e2e-Szenarien, Conformance-Stub.
- Zwei-Serials-Regel bleibt: Pfad = Steuerbox-Serial; HEMS-Identität lebt ausschließlich
  in `ship_id`/`qr_info`.
- Der QR-Info-Datensatz trägt die FNN-Credentials **inkl. SPSEC** — der
  mTLS-Provisioning-Call ist der Secret-Transport zum MSB (ADR-0008 amendiert);
  nie in Logs, Weitergabe an die Box ausschließlich über den Admin-Kanal.