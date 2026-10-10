# ADR-0016: Publicly registered manufacturer identifier instead of self-chosen `manufacturer_id`

- **Status:** Superseded by ADR-0018 (the registered identifier was replaced by the
  IANA PEN of the manufacturer, which also anchors fleet binding)
- **Datum:** 2026-09-25
- **Kontext:** Well-known wire format (amends ADR-0003)

## Kontext

Das Well-known-Dokument trug bisher eine selbstgewählte, nicht registrierte
`manufacturer_id` (freier String). Damit hatte jedes EMS-Hersteller-Backend eine eigene
Kennung ohne öffentliche Bedeutung — Qualifikation und Pin-Store mussten sie bilateral
abstimmen, und es gab keinen herstellerübergreifend verifizierbaren Anker.

## Entscheidung

Das Feld `manufacturer_id` entfällt. Es wird ersetzt durch eine **öffentlich
registrierte Herstellerkennung** — `manufacturer_id` mit Schema-Pattern, so dass
jedes EEBUS-zertifizierte EMS eine global eindeutige Kennung besitzt:

- Die Kennung ist öffentlich bekannt und registriert — keine bilaterale Abstimmung,
  keine Registry in diesem Repo (kein Central-Registry-Prinzip, ADR-0002 bleibt
  unangetastet; die Kennung ist nur ein global eindeutiger **Name**, kein
  Vertrauensanker).

## Konsequenzen

- `spec/wellknown.schema.json`: Feld umbenannt + Pattern.
- Referenz-Server (`src/wellknown/`) und Referenz-Client (`src/ems-backend/`): das
  Struct-Feld heißt wie das Schema-Feld; Log-Metadaten (ADR-0008) nutzen es.
- Kein Verhalten der mTLS-Endpunkte ändert sich; Discovery/Provisioning nutzen die
  herstellerübergreifende Geräte-ID weiter unverändert.
- Trust bleibt an die Client-CA-Pinierung gebunden — die Kennung identifiziert, sie
  autorisiert nicht.

## Ablösung

Die ursprünglich gewählte Kennung (Flag-Kennung des Geräteherstellers nach IEC 62056)
setzte voraus, dass jedes EMS die herstellerübergreifende Identifikationsnummer
verwendet — das ist heute nicht der Fall. ADR-0018 ersetzt die Kennung durch die
**IANA PEN** des Herstellers (`iana_pen` im Well-known-Dokument) und verankert damit
zusätzlich das Fleet-Binding im Provisioning.