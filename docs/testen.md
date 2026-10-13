# Testen & Konformanz

Die öffentlichen E2E-Szenarien und die Black-Box-Konformanzsuite bilden die
Akzeptanzkriterien der Referenzimplementierung. Alle Dienste laufen lokal mit
synthetischen Geräteidentitäten und frisch erzeugten Testzertifikaten.

## Ebenen

| Ebene | Was | Gate |
|---|---|---|
| Unit/Router | Synthetischer Golden-Record-Parser, Regel S, Matching, QR-Decoder, Pin-Store, Fleet-Binding, Store-Verträge | `make test` |
| E2E lokal | Well-known, zwei MSB-Instanzen, HEMS-/Box-Mocks als Prozesse, echte mTLS-Verbindungen | `make e2e` |
| E2E Compose | Dieselben Szenarien mit Containern | `make e2e-docker` |
| Conformance lokal | Alle Black-Box-Tests gegen frischen Referenz-Stack, einschließlich Rate-Limits | `make conformance-local` |
| Conformance extern | Black-Box gegen **beliebige** MSB-Implementierung | `make conformance TARGET=<url>` |
| Spezifikation | OpenAPI-Dateien, externe Referenzen und JSON-Schema-Beispiele | `make spec-validate` |

## E2E-Szenarien (die Suite ist die Spezifikation)

| Szenario | Beweist |
|---|---|
| `HappyPath_ZeroTouch` | Cloud-Beobachtung → Derivation → Fan-out → Provisioning → Parameterisierung → SHIP akzeptiert |
| `WellKnownTrust` | Erste CA-Publikation akzeptiert; abweichender TOFU-Pin verweigert Provisioning |
| `SuffixVPID` / `TxtSerial` | tolerantes Matching + kanonische Serial · exakter Pfad über den TXT-Parser |
| `TriState_Unknown` | MSB down → `unknown`, **nie** `404`; Zero-Touch verweigert auf Unknown |
| `BoxEvidenceMismatch` | falsche Box-SKI → `422 box_mismatch` + Operator-Alarm (niemals still) |
| `RogueClientCA` / `KillSwitch` | Client-Cert außerhalb gepinnter CA → TLS scheitert; Serial-Sperre sofort wirksam |
| `ClientCertRotation` / `ClientCARotation` | Leaf-Rotation ohne Neustart; CA-Rotation über Pin-Store-Reload |
| `RateLimit` / `IdempotentPost` | `429` + wahrheitsgemäßes `Retry-After`; Double-POST → `202/202/409` |
| `AntiEnumeration` | Präfix-Query (kürzer als Serial) → `404` |
| `ErrorContract` / `FleetMismatch` | Maschinenlesbare Fehlercodes; Fremd-Flotten-Payload wird abgelehnt |
| `NetworkBoundaries` | Box-Admin nur mit Token (nur MSB), HEMS-Evidence leakt keine MSB-Koordinaten, falsche SKI koppelt nie |

Beide Betriebsarten sind normativ: `make e2e` (lokale Prozesse) und
`make e2e-docker` (Compose, `deploy/compose`).

Der Runner liegt unter `reference/e2e/` und führt **16 öffentliche Szenarien**
aus. `E2E_SCENARIO=<Name> make e2e` wählt ein Szenario aus; ein unbekannter
Name führt zu einem Fehler. Logs liegen unter `reference/target/e2e-logs/`.

Die Beobachtung der Steuerbox wird über `HEMS_TXT_RECORD` durch den TXT-Parser
eingespeist. Echtes LAN-mDNS und Feldinstallation sind nicht Teil dieses
Backend-Stacks. Der SHIP-Mock prüft TLS plus provisionierte SKI; er implementiert
kein vollständiges EEBUS-SHIP-Protokoll. `NetworkBoundaries` prüft Token/mTLS
und Credential-Trennung, keine physische Netzwerksegmentierung.

## Conformance für Ihre Implementierung

- **MSB:** `make conformance TARGET=https://ihr-msb-staging.example` — black-box, ohne
  Innenleben.
- **EMS-Backend:** die Referenz `reference/ems-backend/` beschreibt das Client-Verhalten; Ihre Implementierung
  muss die e2e-Szenarien gegen einen konformen MSB reproduzieren können.

## Qualitäts-Gates je MR

Die GitHub-Pipeline führt `public-check`, `spec-validate`, `lint`, `test`,
`e2e`, `conformance-local` und den strikten Dokumentationsbuild aus.

- **Lint:** `cargo fmt --check`, `cargo clippy -- -D warnings`, `ruff check` —
  Zero-Warning-Policy, kein `#[allow]` ohne Begründung + Issue/ADR.
- **Öffentlichkeit:** `make public-check` prüft veröffentlichbare Dateien auf
  interne Integrationsmarker, interne Hosts, Gerätebeispiele und Private Keys.
  Der Check ergänzt einen Secrets-Scan; er ist kein Beweis für geheimnisfreie Git-Historie.

Python-Abhängigkeiten: `requirements-dev.txt`; Einrichtung und
Submodul-Verifikation siehe [Entwicklung](entwicklung.md).

## Abdeckungsnachweis

Die öffentliche Suite deckt die vertraglichen Positiv- und Negativfälle ab:

| Eigenschaft | Nachweis |
|---|---|
| Beobachtung → Discovery → Provisioning → Admin-Write → SKI-Kopplung | `HappyPath_ZeroTouch`, `TxtSerial` |
| TOFU-Pin-Mismatch blockiert den Client | `WellKnownTrust` |
| Vollständige VPIDs, Suffixe, URL-Deduplizierung | `SuffixVPID` |
| Nicht erreichbarer MSB und gemischter Fan-out | `TriState_Unknown` |
| Fehlendes Client-Zertifikat, fremde CA | `RogueClientCA`, Conformance mTLS |
| Leaf-/CA-Rotation, Grace, Ablehnung der alten CA danach | `ClientCertRotation`, `ClientCARotation` |
| Falsche/partielle Box-Evidence, Fremd-Flotten, ungültige Payloads | `BoxEvidenceMismatch`, `FleetMismatch`, `ErrorContract`, Conformance |
| Idempotenz, Write-once, Kill-Switch, Anti-Enumeration | Gleichnamige Szenarien und Conformance |
| Discovery-Quota und tatsächlicher Provisioning-Backoff | `RateLimit`, Conformance Rate-Limit |
| Admin-Token, keine Backend-Privatschlüssel im HEMS-Container, falsche SKI trotz gültigem Provisioning | `NetworkBoundaries` |
| QR/SPSEC nicht in Service-/Audit-Logs oder Parser-Fehlern | Happy-Path-Logprüfung und QR-Regressionstest |

Red/Green-Regressionsnachweise bestehen für QR-Fehlertexte, WebPKI ohne
Custom-Root-Bundle und Discovery-Schemata für kurze Queries/lange VPIDs.
Der interne Consumer führt dieselbe öffentliche Library aus und ergänzt
seinen eigenen Inventory-Test; es gibt keine zweite Implementierung der
öffentlichen Szenarien.

`make coverage` benötigt `cargo-llvm-cov` und `llvm-tools-preview`. Es misst
Unit- und lokale Prozess-E2E-Abdeckung und schreibt `reference/target/coverage.json`.
Die Dienste beenden sich auf SIGINT/SIGTERM regulär, damit auch ihre
Profiling-Daten erfasst werden. Gemessene Zeilen-/Funktionsabdeckung und die
Abdeckung der Vertragsfälle sind unterschiedliche Kennzahlen; eine erfolgreiche
Szenario-Suite behauptet keine pauschalen 100 Prozent aller Fehler-/I/O-Pfade.

Bugfix-Regel: ein Fix ohne Regressionstest, der ohne den Fix fehlschlägt, ist kein Bugfix.
