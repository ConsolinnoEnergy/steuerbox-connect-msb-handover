# ADR-0016: DLMS manufacturer flag id instead of self-chosen `manufacturer_id`

- **Status:** Accepted
- **Datum:** 2026-09-25
- **Kontext:** Well-known wire format (amends ADR-0003)

## Kontext

Das Well-known-Dokument trug bisher eine selbstgewählte, nicht registrierte
`manufacturer_id` (freier String). Damit hatte jedes EMS-Hersteller-Backend eine eigene
Kennung ohne öffentliche Bedeutung — Qualifikation und Pin-Store mussten sie bilateral
abstimmen, und es gab keinen herstellerübergreifend verifizierbaren Anker.

## Entscheidung

Das Feld `manufacturer_id` entfällt. Es wird ersetzt durch
**`dlms_manufacturer_id`** — die öffentlich registrierte DLMS-Hersteller-Flag-ID
(IEC 62056), die jedes EEBUS-zertifizierte EMS ohnehin besitzt:

- Format: exakt drei Großbuchstaben, Schema-Pattern `^[A-Z]{3}$`.
- Beispiel: **`CEG`** als Flag-ID eines Beispiel-Herstellers.
- Die Kennung ist öffentlich bekannt und vom DLMS UA registriert — keine bilaterale
  Abstimmung, keine Registry in diesem Repo (kein Central-Registry-Prinzip, ADR-0002
  bleibt unangetastet; die Kennung ist nur ein global eindeutiger **Name**, nicht ein
  Vertrauensanker).

## Konsequenzen

- `spec/wellknown.schema.json`: Feld umbenannt + Pattern; Beispiel auf `CEG`.
- Referenz-Server (`src/wellknown/`): Env `WK_DLMS_MANUFACTURER_ID` (Default `CEG`).
- Referenz-Client (`src/ems-backend/`): Struct-Feld `dlms_manufacturer_id`; Log-Metadaten
  (ADR-0008) nutzen `dlms_manufacturer_id`.
- Kein Verhalten der mTLS-Endpunkte ändert sich; Discovery/Provisioning nutzen die
  herstellerübergreifende Geräte-ID weiter unverändert.
- Trust bleibt an die Client-CA-Pinierung gebunden — die DLMS-ID identifiziert, sie
  autorisiert nicht.