# ADR-0018: IANA PEN instead of DLMS flag id — fleet binding on provisioning

- **Status:** Accepted (supersedes ADR-0016; DLMS flag id removed for now)
- **Datum:** 2026-10-10
- **Kontext:** Well-known wire format (amends ADR-0003), provisioning error taxonomy,
  fleet protection

## Kontext

ADR-0016 hatte `dlms_manufacturer_id` (DLMS-Flag-ID, IEC 62056) als Identifikationsanker
eingeführt. Die DLMS-Flag-ID setzt voraus, dass jedes EMS die
herstellerübergreifende Identifikationsnummer verwendet — das ist heute nicht der Fall:
EMS-Hersteller fahren ihre eigene, zulässige Seriennummernformat-Flotte
(z. B. `1U0022_CO-4206` als VPID in der SHIP-ID).

Die SHIP-ID bleibt vom Format her `i:<IANA-PEN>_u:<VPID>` — der `i:`-Teil trägt die
**IANA PEN des Geräteherstellers** (Consolinno: **12345**), der `_u:`-Teil die
herstellereigene Seriennummer (VPID).

## Entscheidung

1. **`dlms_manufacturer_id` entfällt** aus dem Well-known-Dokument.
2. **`iana_pen`** kommt stattdessen ins Well-known-Dokument
   (`spec/wellknown.schema.json`, Pattern `^[0-9]{1,7}$`, Beispiel `12345`).
3. **Fleet binding (normativ):** die IANA PEN im Well-known-Dokument MUSS mit der
   IANA PEN in den SHIP-IDs übereinstimmen, die das EMS-Backend in den
   Provisioning-Payloads meldet (`pairing_dataset.ship_id`, `i:<PEN>_u:<VPID>`).
   Ein Mismatch ist **`422 fleet_mismatch`** + Operator-Alarm — nie ein stiller Ignore.

Damit hat die MSB eine zusätzliche Absicherung: ein EMS-Anbieter kann nur
Pairing-Daten für seine **eigene Flotte** übermitteln — die gepinnte Client-CA
identifiziert den Hersteller, die PEN bindet die Payload an seine Flotte.

## Konsequenzen

- `spec/wellknown.schema.json`: `dlms_manufacturer_id` → `iana_pen` (Pattern
  `^[0-9]{1,7}$`).
- `spec/provisioning.yaml`: neuer Fehlercode `422 fleet_mismatch`.
- Referenz-Server (`src/wellknown/`): Env `WK_IANA_PEN` (Default `12345`).
- Referenz-MSB (`src/msb/`, `msb-core`): `MsbState.iana_pen`; Provisioning prüft die
  PEN in `pairing_dataset.ship_id` gegen die konfigurierte Well-known-PEN —
  Mismatch → `422 fleet_mismatch` + Operator-Alarm. Env `MSB_IANA_PEN`.
- Referenz-Client (`src/ems-backend/`): Struct-Feld `iana_pen`; der Client prüft
  zusätzlich client-seitig, dass die HEMS-SHIP-ID die Well-known-PEN trägt, bevor er
  provisioniert (Defense-in-Depth; der definitive Check bleibt server-seitig).
  Log-Metadaten (ADR-0008) nutzen `iana_pen`.
- ADR-0016 ist damit gesuperseded; die Qualifikation (ADR-0002) bleibt bilateral,
  Anker ist nun die IANA PEN.
- Trust bleibt an die Client-CA-Pinierung gebunden — die PEN identifiziert die
  Flotte, sie autorisiert nicht.
- Neues e2e-Scenario `FleetMismatch` + conformance-Case `test_fleetbinding.py`.
