# Für Messstellenbetreiber

Was Sie als MSB implementieren und betreiben, um Zero-Touch-Pairing mit
EMS-Herstellern zu ermöglichen.

## Ihre Komponenten

| Komponente | Wo | Normativ? |
|---|---|---|
| Discovery-Endpunkt `GET /v1/serials/{serial\|vpid}` (mTLS) | Ihr Backend | Ja — `spec/` |
| Provisioning-Endpunkt `POST /v1/serials/{serial}/provisioning` (mTLS) | Ihr Backend | Ja |
| Client-CA-Pin-Store (TOFU + TTL + Rotation-Grace) | Ihr Backend | Ja |
| Portfolio-Serial-Store, toleranter Matcher | Ihr Backend | Ja — [Regeln](../spec/index.md#matching-regeln) |
| Parameterisierung Steuerbox via SMGW | SMGW/Box-Admin oder TAF | Prozesskontext |
| §12-EnWG-Steuerungsnachweis (pEMT/SMGW-Messwerte) | Ihr Betrieb | Prozesskontext |

## 1. Zwei mTLS-Endpunkte

Beide Endpunkte sind **mTLS-only** — das Client-Zertifikat wird gegen die gepinnte
Client-CA des jeweiligen EMS-Herstellers geprüft. Die CA pflegt der MSB in seinem
Pin-Store (Quelle: Well-known-Publikation des Herstellers, base64-PEM + Gültigkeits-
fenster, ADR-0013); der Well-known-Fetch erfolgt clientseitig im EMS-Backend.

**Discovery** `GET /v1/serials/{serial|vpid}`:

- Frage: „Gehört diese herstellerübergreifende ID zu diesem MSB?"
- Status-code basiert (kein Body): `200` = found, `404` = not found. Die kanonische
  Serial leitet der Client selbst ab (Regel-S-Basis der Query, ADR-0012).
- **Anti-Enumeration:** Query kürzer als eine Portfolio-Serial → `404`.
- Matching toleriert die Query-**Form** (Regel-S-Basis, `-`-Suffix, VPID-Form), ist
  aber **präfixlos**: exakte Basis-Gleichheit (ADR-0012).
- Tri-state-korrekt: Transportfehler = `unknown` (nie `404` melden).

**Provisioning** `POST /v1/serials/{serial}/provisioning`:

- Write **nur** auf der kanonischen Serial aus der Discovery-Antwort; client-abgeleitete
  Serials ablehnen.
- Payload: `pairing_dataset` (ship_id, SKI, `qr_info` = SHIP-Info-Datensatz **inkl.**
  SPSEC-Secret — der mTLS-Provisioning-Call ist sein Transport, nie in Logs, ADR-0017)
  + optionales `box_evidence` (beobachtete Box-ship_id + SKI). HEMS-Serial, FPH und
  base-URI sind in den Feldern enthalten (Serial steckt im VPID-Teil der ship_id,
  FPH im QR-String, base-URI ist bilateral in der Qualifikation fixiert).
- `box_evidence` mit Inventar abgleichen: Mismatch → `422 box_mismatch` + Operator-Alarm —
  niemals still ignorieren.
- Idempotent auf `(serial, ship_id)`; Antwort `202 Accepted` (Closed Loop beweist erst der
  SHIP-Handshake).
- Write-once: Serial↔MSB ändert sich nie.

Payload-/Response-Details: [REST-API-Referenz](../api.md).

## 2. Qualifikation & Pinning

1. Bilaterale Qualifikation je EMS-Hersteller (DLMS-Flag-ID als Anker): Sie erhalten die
   well-known URI, teilen API-Basis-URL, die zwei Endpunkte und Quoten mit.
2. Sie halten die Client-CA in Ihrem Pin-Store (TOFU + TTL; Quelle: Well-known-Publikation
   des Herstellers bzw. die in der Qualifikation geteilte CA).
3. Rotation: Grace-Fenster im Pin-Store (Alt+Neu parallel, ohne Neustart); das
   Well-known-Dokument trägt das Gültigkeitsfenster der aktuellen CA (ADR-0013);
   Serial-Kill-Switch sofort wirksam (Szenario `KillSwitch`).
4. mTLS terminiert **in Ihrer Anwendung** — kein Edge-Proxy-Termination (k8s:
   TLSRoute/ssl-passthrough, damit der Pod das Client-Cert sieht).

## 3. Parameterisierung der Steuerbox

Nach akzeptierter Provisionierung schreiben Sie die EEBUS-Daten des EMS auf die Steuerbox:

- **über den Steuerbox-Admin**, oder
- **bei SMGWv2 via TAF** am SMGW,

und richten die Steuerung über EEBUS ein. Das Repo liefert dafür nur den
Prozesskontext und den Admin-Kanal-Adapter-Trait in der Referenz (`src/msb/`); die
tatsächliche SMGW-/TAF-Integration ist Ihr Teil.

Der Secret-Transport ist zweiteilig (ADR-0008): Der QR-Info-Datensatz **inkl.
SPSEC-Security-Code** erreicht Sie über den mTLS-Provisioning-Call — das ist sein
Transport, **niemals in Logs** (Provisioning-Bodies werden nie geloggt, nur Metadaten).
Weitergeben tun Sie die FNN-Credentials ausschließlich über Ihren Box-Admin-Kanal
(Steuerbox-Admin / TAF am SMGWv2) — nie über eine andere API.

## 4. Steuerungsnachweis nach §12 EnWG

Regelmäßiger Test für den Steuerungsnachweis durch den MSB in Kombination mit
pEMT-/SMGW-Messwert-Auswertung. Außerhalb dieses Repos — der nachweisbare EEBUS-Regelbetrieb
wird durch den Closed Loop (SHIP-Handshake im LAN) vorbereitet.

## 5. Referenz-Implementierung einbinden (`msb-core`)

Die Referenz-Logik ist als **Library** (`msb-core`, ADR-0015) nutzbar — Sie implementieren
nur die DB-Schnittstelle und binden Router/Handler in Ihr Backend ein:

```toml
[dependencies]
msb-core = "0.1"
```

```rust
use msb_core::{api::MsbState, store::{MsbStore, InventoryEntry, PairingRecord, ProvisionOutcome}};

// 1) MsbStore gegen Ihre echte Portfolio-/Inventar-DB implementieren (5 Methoden):
//    lookup (tolerantes Matching + Anti-Enumeration), is_killed/kill (Kill-Switch),
//    in_portfolio, inventory (erwartete Box-Evidence), provision (idempotent, write-once).
//    Semantik normativ: matcher-Hilfen aus msb_core::matcher nutzen (ADR-0004/-0012).

// 2) State + Router in Ihr bestehendes Backend einhängen:
let state = MsbState::new(
    Arc::new(MyDbStore::new(pool)),  // Ihre DB
    "ihr-msb-id", 1000,               // msb_id, Rate-Limit (429 + Retry-After bleibt Kontrakt)
    Arc::new(MyBoxAdmin::new()),      // Ihr Box-Admin-Kanal (trait BoxAdminAdapter)
    "admin-token",
);
let app = msb_core::api::api_router(Arc::new(state));
// mTLS terminiert in Ihrer Anwendung (ADR-0009) — eigenen mTLS-Server vor den Router
// setzen oder msb_core::api::serve() mit .with_tls(...) nutzen (Pin-Store-Dir, Rotation-Grace).
```

Referenz-Dummy für Tests/Start: `InMemoryStore` (gleicher Trait); der Referenz-Stack
(`msb`-Binary, Compose, e2e) läuft exakt über denselben Codepfad. Geprüft wird Ihre
Einbindung black-box über die Conformance-Suite (Abschnitt 6) — der Trait-Vertrag allein
befreit nicht von den normativen Regeln (Tri-State, Write-once, `422 box_mismatch`,
kein Existenz-Leak).

## 6. Konformanz nachweisen

```bash
make conformance TARGET=https://ihr-msb-staging.example
```

Geprüft (black-box): mTLS erzwungen, Fehlertaxonomie, Tri-State, tolerantes Matching +
Anti-Enumeration, Idempotenz, `429` + `Retry-After`, `422 box_mismatch`, Secret-Freiheit,
Write-once. Die Referenz-Implementierung (`src/msb/`) besteht dieselbe Suite in CI.