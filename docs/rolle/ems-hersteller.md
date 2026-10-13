# Für EMS-Hersteller

Was Sie als EMS-Hersteller implementieren müssen, um das Zero-Touch-Pairing Ihrer
HEMS-Geräte mit Steuerboxen zu ermöglichen.

## Ihre Komponenten

| Komponente | Wo | Normativ? |
|---|---|---|
| Well-known-Endpunkt (Client-CA-Publikation) | bei Ihnen | Ja — `spec/wellknown.schema.json` |
| Client-CA (dedizierte Sub-CA, `scope: steuerbox-connect-api`) | bei Ihnen | Ja |
| EMS-Backend (Discovery + Provisioning-Client) | bei Ihnen | Ja — Referenz: `reference/ems-backend/` |
| HEMS-Gerät (LAN-Erkennung, Gerätee-Token) | bei Ihnen | Referenz-Verhalten: `reference/hems-mock/` |

## 1. Well-known-Endpunkt bereitstellen

Veröffentlichen Sie `GET /.well-known/steuerbox-connect/v1` (HTTPS, öffentlich) mit
drei Feldern:

| Feld | Pflicht | Bedeutung |
|---|---|---|
| `iana_pen` | ja | Ihre IANA PEN (z.B. `12345`); MUSS mit der PEN in Ihren SHIP-IDs übereinstimmen (ADR-0018) |
| `client_ca` | ja | Base64-PEM Ihrer dedizierten Client-Sub-CA (`scope: steuerbox-connect-api`) + Gültigkeitsfenster `not_before`/`not_after` (ADR-0013) |
| `contact` | ja | Kontakt für MSBs (Rotation, Störungen) |

Vollständiges Beispiel und Feldtabelle: [REST-API-Referenz](../api.md#1-get-well-knownsteuerbox-connectv1).
Pflichten: Verfügbarkeit, signierte Rotation (Grace-Fenster im Dokument), keine
Private-Keys im Dokument. Die Steuerbox-Serial Ihres Portfolios steckt **nicht** hier —
die kennt nur der MSB.

## 2. Client-Zertifikate für Ihr Backend

- Das EMS-Backend nutzt ein Client-Cert, das von Ihrer Hersteller-Sub-CA signiert wird.
- Dediziertheit ist Pflicht: Die CA darf ausschließlich für
  `steuerbox-connect-api`-Client-Certs verwendet werden (kein Webservice-Zweck).
- Rotation ist autonom; über das Well-known-Dokument veröffentlichen Sie das
  Grace-Fenster, in dem alte und neue CA parallel gültig sind.

## 3. Qualifikation je MSB

Für jeden MSB, mit dem Sie koppeln wollen:

1. Bilaterale Qualifikation zur EEBUS-Kopplung und -Steuerung (IANA PEN als Anker, ADR-0018).
2. Sie teilen Ihre well-known URI mit.
3. Sie erhalten: API-Basis-URL, die zwei mTLS-Endpunkte (Discovery + Provisioning) und
   Ihre Quoten. Konfigurieren Sie diese im Backend — nie im Code hart verdrahten.

## 4. EMS-Backend-Implementierung

Das Backend implementiert die Client-Seite der Spec. Verhaltensregeln (normativ,
getestet durch die e2e-Suite):

- **Gerätee-Token-Auth vom HEMS-Gerät:** Das HEMS meldet nach der LAN-Erkennung der
  Steuerbox seine EEBUS-Daten an Ihr Backend — authentifiziert mit Gerätee-Token
  (Ihr Verfahren, z.B. JWT — **außerhalb dieses Repos**, hier nicht normativ).
- **Discovery-Aufruf:** `GET /v1/serials/{id}` mit der herstellerübergreifenden ID aus der
  ship_id der erkannten Steuerbox. **Tri-State einhalten:** `200` = found, `404` =
  not found (kanonische Serial = Regel-S-Basis der Query), Transportfehler = `unknown`,
  niemals `404`; auf `unknown` kein Provisioning.
- **Provisioning-Aufruf:** `POST /v1/serials/{canonical_serial}/provisioning` — nur auf
  der kanonischen Serial aus der Discovery-Antwort, nie auf einer selbst abgeleiteten.
  Payload: `pairing_dataset` (ship_id, SKI, `qr_info` inkl. SPSEC-Secret — mTLS ist der
  Transport, nie in Logs, ADR-0017) + `box_evidence` (beobachtete Box-ship_id + SKI).
- **Idempotenz:** Wiederholte POSTs auf `(serial, ship_id)` sind ohne Schaden.
- **Rate Limits:** `429` + `Retry-After` müssen eingehalten werden.
- **Fan-out über mehrere MSB-Plattformen:** mit URL-Dedupe; paralleler Fan-out ist
  erlaubt, Ergebnis fan-out-weit tri-state zusammenführen.

Beispiel-Requests/-Responses für beide Endpunkte:
[REST-API-Referenz](../api.md).

Kein SHIP-Secret, niemals, in keinem Payload, Log oder Beispiel — das Secret reist
ausschließlich über den Admin-Kanal des MSB.

## 5. HEMS-Gerät (Kunden-LAN)

- mDNS-Browse auf `_ship._tcp`, Filter Gateway-Klasse: ship_id + SKI der Steuerbox.
- Serial-Derivation nach **Regel S** aus dem TXT-Record (`id=VPID`, z. B.
   `i:51593_u:SNTESTGCPH0001`): split am ersten `-`, keine Hyphen in kanonischen Serials.
- Meldung an Ihr Backend: eigene EEBUS-Daten + Box-ship_id (enthält die
  herstellerübergreifende ID) + Box-SKI, mit Gerätee-Token authentifiziert.
- Nach der Bestätigung des Provisioning-Status: SHIP-Retry-Loop gegenüber der Steuerbox —
  der akzeptierte SHIP-Handshake ist der Closed-Loop-Beweis.

## 6. Installationsprozess

Geräteseitige HEMS-Implementierung und die komplette Kommunikation HEMS ↔ EMS-Backend
(Gerätee-Token, LAN-Beobachtung, mDNS, SHIP im LAN) laufen **außerhalb dieses Repos** — das
Repo deckt nur die Backend-zu-Backend-Seite (EMS-Backend ↔ MSB-API) ab. Es wird hier nichts
installiert. Sie melden die Kundenanlage fertig über den Installateursbetrieb; erst danach
beginnt der Zero-Touch-Bootstrap (s. [Prozess](../prozess.md), Schritte 4–8).

## Testen

- Gegen die Referenz: `make e2e` und `make conformance TARGET=<msb-staging-url>`.
- Die Black-Box-Suite (`conformance/`) prüft *Ihre* Client-Implementierung nicht direkt —
  sie prüft MSBs. Für Ihre Client-Seite gilt die e2e-Suite als Akzeptanzkriterium; die
  Referenz-Verhalten: `reference/ems-backend/`.
