# Enrollment — Qualifikation, Pinning, Rotation

Das Enrollment ist der einmalige, bilaterale Vertrauens-Aufbau zwischen einem
MSB und einem EMS-Hersteller. Normative Basis: `spec/wellknown.schema.json`.
Referenz: `src/wellknown/` (Server), `src/ems-backend/` (Client-Konfiguration),
`src/msb/` (Pin-Store).

## Qualifikations-Handshake

Voraussetzung der Kopplung ist eine einmalige **Qualifikation** zwischen MSB und
EMS-Hersteller zur EEBUS-Kopplung und -Steuerung. Der Anker sind die
**IANA PEN** (z.B. `12345`, ADR-0018) — kein zentrales Register, keine bilateralen Credential-Stores.

Austausch nach der Qualifikation:

| Richtung | Inhalt |
|---|---|
| Hersteller → MSB | Well-known-URI (stabil, HTTPS) |
| MSB → Hersteller | API-Basis-URL, zwei mTLS-Endpunkte (Discovery + Provisioning), Quoten |

Ergebnis: Der MSB kennt die Client-CA-Quelle des Herstellers; das Hersteller-Backend
kennt die MSB-Koordinaten. Beides lebt in **Konfiguration**, nie hart im Code.

## Well-known-Dokument

`GET /.well-known/steuerbox-connect/v1` (öffentlich, WebPKI genügt):

```json
{
  "iana_pen": "12345",
  "client_ca": {
    "pem": "LS0tLS1CRUdJTiBDRVJUSUZJQ0FURS0tLS0tCg==",
    "scope": "steuerbox-connect-api",
    "not_before": "2026-09-25T00:00:00Z",
    "not_after": "2026-10-25T00:00:00Z"
  },
  "contact": "https://…"
}
```

`client_ca.pem` ist **base64-kodiert** (ADR-0013); `not_before`/`not_after` (RFC 3339)
sind das Gültigkeitsfenster der aktuellen CA — Verbraucher lehnen Dokumente außerhalb
des Fensters hart ab.

Pflichten des Herstellers:

- Stabile URL, hohe Verfügbarkeit; Inhalt **signiert/integritätsprüfbar** halten.
- Kein Private-Key im Dokument — nur der öffentliche CA-Teil, base64-kodiert.
- Bei Rotation: neues `client_ca` mit aktualisiertem Gültigkeitsfenster. Das Dokument
  modelliert **ein** CA-Fenster; „Alt und Neu parallel gültig" (Grace) lebt im
  Pin-Store des MSB, nicht im Dokument (ADR-0013).

## Pinning (MSB-Seite)

1. **Pins:** Der MSB hält die öffentliche(n) Client-CA(s) in einem beobachteten
   Pin-Store (lokal, `MSB_CLIENT_CA_DIR`); der Well-known-Fetch erfolgt auf der
   EMS-Backend-Seite (ADR-0003/0013).
2. **TOFU:** Die erste beobachtete CA wird als Pin übernommen.
3. **Validierung:** Jedes mTLS-Client-Cert wird gegen die gepinnten CAs geprüft; das
   Cert muss von der dedizierten Hersteller-Sub-CA signiert sein
   (`scope: steuerbox-connect-api` — keine Webservice-Zwecke).
4. **Rotation:** Während des Grace-Fensters sind Alt- und Neu-CA im Pin-Store
   akzeptiert (Pin-Store-Reload, ohne Neustart); danach wird die alte CA verworfen.
5. **Kill-Switch:** Der Serial-Kill-Switch ist sofort wirksam (e2e: `KillSwitch`) —
   kein Grace-Fenster für Sperren; die Antwort leakt nicht, dass die Serial je
   existierte. Eine kompromittierte CA wird durch sofortige Entfernung aus dem
   Pin-Store abgewiesen (kein Grace für Entfernung).

## Rotation demokratisch getestet

Normative Szenarien der e2e-Suite:

| Szenario | Beweist |
|---|---|
| `ClientCertRotation` | Leaf-Rotation ohne Neustart |
| `ClientCARotation` | CA-Rotation über Pin-Store-Reload + Grace-Fenster |
| `RogueClientCA` | Client-Cert außerhalb der gepinnten CA scheitert am TLS-Handshake |
| `KillSwitch` | gesperrte **Serial** wird sofort abgewiesen (Discovery `404`, Provisioning `404 not_found`, kein Existenz-Leak) |

## Invarianten

- Der MSB validiert Client-Certs gegen die CAs in seinem Pin-Store; die CA wird vom
  EMS-Hersteller per Well-known-Dokument publiziert (der Fetch erfolgt clientseitig im
  EMS-Backend, der Pin des MSB kommt aus der Qualifikation) — es gibt keine manuelle
  Cert-Verteilung.
- mTLS terminiert **in der Anwendung**; in k8s via TLSRoute/ssl-passthrough, damit der
  Pod das Client-Cert sieht (Edge-Termination ist eine Hard-Rule-Verletzung).
- Der Anwendungscode verzweigt nie auf die ausstellende CA; Umgebungsunterschiede
  (sim-LE lokal, Let's Encrypt/Hersteller-Sub-CA produktiv) leben nur in Konfiguration.