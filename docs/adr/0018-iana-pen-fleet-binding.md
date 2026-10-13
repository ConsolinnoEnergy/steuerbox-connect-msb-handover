# ADR-0018: IANA PEN instead of the registered manufacturer identifier — fleet binding on provisioning

- **Status:** Accepted (supersedes ADR-0016)
- **Datum:** 2026-10-10
- **Kontext:** Well-known wire format (amends ADR-0003), provisioning error taxonomy,
  fleet protection

## Kontext

ADR-0016 hatte eine öffentlich registrierte Herstellerkennung im Well-known-Dokument
verankert, die auf der Flag-Kennung des Geräteherstellers nach IEC 62056 basierte.
Diese Kennung setzt voraus, dass jedes EMS die herstellerübergreifende
Identifikationsnummer verwendet — das ist heute nicht der Fall: EMS-Hersteller fahren
ihre eigene, zulässige Seriennummernformat-Flotte (z. B. `SNTEST_HEMS-0001` als VPID in
der SHIP-ID).

Die SHIP-ID bleibt vom Format her `i:<IANA-PEN>_u:<VPID>` — der `i:`-Teil trägt die
**IANA PEN des Geräteherstellers** (synthetisches Beispiel: **12345**), der `_u:`-Teil die
herstellereigene Seriennummer (VPID).

## Entscheidung

1. Das Feld `manufacturer_id` aus ADR-0016 entfällt aus dem Well-known-Dokument.
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

- `spec/wellknown.schema.json`: `manufacturer_id` → `iana_pen` (Pattern
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
