# REST-API-Referenz

Die drei Endpunkte des Zero-Touch-Pairings — alle Antworten als JSON, Fehler immer
mit `error` (Maschinen-Code) und `message` (menschlich lesbar).

Download der OpenAPI-Spec: [`docs/api/openapi.json`](api/openapi.json) —
direkt in [Swagger UI](https://editor.swagger.io) oder Redoc öffnen.

| Endpunkt | Methode | Wer | Auth |
|---|---|---|---|
| `/.well-known/steuerbox-connect/v1` | `GET` | EMS-Hersteller | Server-TLS (öffentlich) |
| `/v1/serials/{serial}` | `GET` | MSB | **mTLS-only** |
| `/v1/serials/{serial}/provisioning` | `POST` | MSB | **mTLS-only** |

---

## 1. `GET /.well-known/steuerbox-connect/v1`

Well-known-Dokument des EMS-Herstellers. Kein Request-Body, keine Authentifizierung.

**Antwort `200`:**

```json
{
  "iana_pen": "12345",
  "client_ca": {
    "pem": "LS0tLS1CRUdJTiBDRVJUSUZJQ0FURS0tLS0tCg==",
    "scope": "steuerbox-connect-api",
    "not_before": "2026-09-25T11:17:03Z",
    "not_after": "2026-10-25T11:17:03Z"
  },
  "contact": "ops@example-ems.example"
}
```

| Feld | Typ | Pflicht | Bedeutung |
|---|---|---|---|
| `iana_pen` | string, `^[0-9]{1,7}$` | ja | IANA PEN des Herstellers (z.B. `12345`); Fleet-Binding, ADR-0018 |
| `client_ca.pem` | string (base64-PEM) | ja | Dedizierte Client-Sub-CA, die die mTLS-Client-Certs des EMS-Backends signiert |
| `client_ca.scope` | string | ja | Immer `steuerbox-connect-api` |
| `client_ca.not_before` / `not_after` | date-time | nein | Gültigkeitsfenster (Rotation-Grace) |
| `contact` | string | ja | Kontakt für MSBs (Rotation, Störungen) |

**Antwort `404`:** Dokument an dieser URL nicht veröffentlicht.

---

## 2. `GET /v1/serials/{serial}` — Discovery (MSB, mTLS)

Frage: „Gehört diese Serial bzw. herstellerübergreifende ID zu diesem MSB?"
Die Abfrage darf tolerant sein (Regel-S-Basis, VPID-Form) — die **kanonische Serial**
leitet der Client selbst aus der Query ab (Regel-S-Basis: Split am ersten `-`,
deterministisch, ADR-0012); der **Schreib**-Aufruf braucht immer sie.

**Request:** Pfadparameter `{serial}` (string, 4–32 Zeichen), kein Body.

**Antworten (status-code basiert, kein JSON-Body):**

| Status | Bedeutung |
|---|---|
| `200` | Found — Serial gehört zu diesem MSB; leerer Body |
| `404` | Not found — unbekannte Serial, Anti-Enumeration (Query kürzer als eine Portfolio-Serial) oder Kill-Switch; leerer Body, kein Existenz-Leak |
| `429` | Rate-Limit; Header `Retry-After` (Sekunden) ist Pflicht, Body `{"error": "rate_limited", …}` |

**Regeln:**

- Kein Body bei `200`/`404` — der Kanal ist nur der Statuscode.
- Transportfehler sind serverseitig nicht sichtbar; clientseitig gilt: Transportfehler,
  TLS-Ablehnung oder anderer Status ⇒ `unknown`, nie `404` (Tri-State, ADR-0005).

---

## 3. `POST /v1/serials/{serial}/provisioning` — Provisioning (MSB, mTLS)

Übermittelt die EEBUS-Daten des EMS für die Steuerbox `{serial}`.
**Write nur auf der kanonischen Serial aus der Discovery-Antwort.**

**Request:** Pfadparameter `{serial}` (string, `^[A-Za-z0-9]{4,32}$`), JSON-Body:

```json
{
  "pairing_dataset": {
    "ship_id": "i:0002_u:1U0022_CO-4206",
    "ski": "8pGzaOi1OYDGG0ZFrPMyXjja9dM=",
    "qr_info": "SHIP;SKI:6ADFA10E…;ID:ExampleEMS-Leaflet-HEMS-1u0022-co-4206;BRAND:ExampleEMS;TYPE:EnergyManagementSystem;MODEL:Leaflet-HEMS;SERIAL:1u0022-co-4206;CAT:2;FPH256:7EFB1B40…;SPSEC:SPSEC-EXAMPLE-REPLACE-IN-PROD;ENDSHIP;"
  },
  "box_evidence": {
    "ship_id": "i:0001_u:SNTEST0001",
    "ski": "6gTHbL2v3fX3yZaJ1x8vN0dE="
  }
}
```

| Feld | Typ | Pflicht | Bedeutung |
|---|---|---|---|
| `pairing_dataset.ship_id` | string | ja | HEMS ship_id `i:<IANA-PEN>_u:<VPID>` — die HEMS-Serial steckt im VPID-Teil |
| `pairing_dataset.ski` | string | ja | SKI des HEMS |
| `pairing_dataset.qr_info` | string, `^SHIP;.+ENDSHIP;$` | ja | SHIP-Info-Datensatz aus dem QR-Code, **inkl. SPSEC-Security-Code** — mTLS ist der Transport, niemals in Logs (ADR-0008/-0017) |
| `box_evidence.ship_id` / `.ski` | string | nein | In-LAN beobachtete Steuerbox (empfohlen; Server gleicht mit Inventar ab) |

**Antwort `202` (erfolgreich):**

```json
{ "status": "accepted" }
```

Idempotent auf `(serial, ship_id)` — Wiederholungen liefern ebenfalls `202`.

**Fehler (JSON mit `error` + `message`):**

| Status | `error` | Bedeutung |
|---|---|---|
| `400` | `not_canonical` | Pfad-Serial ist nicht die kanonische Serial aus der Discovery |
| `404` | `not_found` | Serial nicht im Portfolio oder gekillt (Kill-Switch) |
| `409` | `write_once_conflict` | Serial ist bereits an eine andere ship_id gepaart (Write-once) |
| `422` | `box_mismatch` | `box_evidence` passt nicht zum Inventar |
| `422` | `invalid_pairing_dataset` | `pairing_dataset` unvollständig/ungültig |
| `422` | `invalid_body` | Body ist kein gültiger Provisioning-Request |
| `429` | `rate_limited` | Rate-Limit; `Retry-After`-Header (Pflicht) einhalten |

---

## Quellen

Normativ sind die OpenAPI-/JSON-Specs unter
`spec/`
(`discovery.yaml`, `provisioning.yaml`, `wellknown.schema.json`,
`pairing_dataset.schema.json`, `examples/`). Diese Seite ist die vereinfachte
Entwickler-Sicht; bei Konflikt gilt die Spec. Validierung: `make spec-validate`.