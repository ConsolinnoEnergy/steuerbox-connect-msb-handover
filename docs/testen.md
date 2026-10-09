# Testen & Konformanz

Das Repo ist **testgetrieben end-to-end** — jedes Verhalten existierte zuerst als
failing test. Die Testsuite ist damit die normative Akzeptanzkriterium-Menge für jede
Implementierung.

## Ebenen

| Ebene | Was | Gate |
|---|---|---|
| Unit | Golden-Record-Parser (maskierte echte mDNS-TXT-Dumps), Regel-S-Randfälle, Matcher-Semantik, OpenAPI-Validierung, QR-Decoder (SHIP-Info-Datensatz, SPSEC nie geloggt) | `make test` |
| Integration | In-Memory-TLS-Paare: Pin-Store, Rotation-Grace, Quoten-Logik | `make test` |
| E2E | Vollständiger Compose-Stack, echte Zertifikate aus `test/ca`, echtes mDNS per Default | `make e2e` |
| Conformance | Black-Box gegen **beliebige** MSB-Implementierung | `make conformance TARGET=<url>` |

## E2E-Szenarien (die Suite ist die Spezifikation)

| Szenario | Beweist |
|---|---|
| `HappyPath_ZeroTouch` | Cloud-Beobachtung → Derivation → Fan-out → Provisioning → Parameterisierung → SHIP akzeptiert |
| `SuffixVPID` / `TxtSerial` | tolerantes Matching + kanonische Serial · exakter Pfad über den TXT-Parser |
| `TriState_Unknown` | MSB down → `unknown`, **nie** `404`; Zero-Touch verweigert auf Unknown |
| `BoxEvidenceMismatch` | falsche Box-SKI → `422 box_mismatch` + Operator-Alarm (niemals still) |
| `RogueClientCA` / `KillSwitch` | Client-Cert außerhalb gepinnter CA → TLS scheitert; Serial-Sperre sofort wirksam |
| `ClientCertRotation` / `ClientCARotation` | Leaf-Rotation ohne Neustart; CA-Rotation über Pin-Store-Reload |
| `RateLimit` / `IdempotentPost` | `429` + wahrheitsgemäßes `Retry-After`; Double-POST → `202/202/409` |
| `AntiEnumeration` | Präfix-Query (kürzer als Serial) → `404` |
| `NetworkBoundaries` | Box-Admin nur mit Token (nur MSB), HEMS-Evidence leakt keine MSB-Koordinaten, falsche SKI koppelt nie |

Beide Betriebsarten sind normativ: `make e2e` (lokale Prozesse) und
`make e2e-docker` (Compose, `deploy/compose`).

## Conformance für Ihre Implementierung

- **MSB:** `make conformance TARGET=https://ihr-msb-staging.example` — black-box, ohne
  Innenleben.
- **EMS-Backend:** die Referenz `src/ems-backend/` ist normativ; Ihre Implementierung
  muss die e2e-Szenarien gegen einen konformen MSB reproduzieren können.

## Qualitäts-Gates je MR

Pfad: `lint → spec-validate → test → e2e → conformance → audit/deny → gitleaks`.

- **Lint:** `cargo fmt --check`, `cargo clippy -- -D warnings`, `ruff check` —
  Zero-Warning-Policy, kein `#[allow]` ohne Begründung + Issue/ADR.
- **Vulnerabilities:** `cargo audit` + `cargo deny` + `pip-audit` — blocking.
- **Secrets:** `gitleaks`-History-Scan — fängt CA-Material, Client-Keys, Pin-Stores,
  echte Serials (nur `SNTEST*`-Masken erlaubt).
- **SAST:** GitLab-SAST; Funde wie Bugs triagen.

Bugfix-Regel: ein Fix ohne Regressionstest, der ohne den Fix fehlschlägt, ist kein Bugfix.