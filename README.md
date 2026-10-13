# Steuerbox-Connect — MSB-Implementierungspaket

## Präambel

Dieses Paket ist eine **Diskussionsgrundlage und Vorlage** für alle Akteure der Branche —
Messstellenbetreiber (MSB) und EMS-Hersteller gleichermaßen. Es beschreibt einen
offenen, bilateralen Zero-Touch-Bootstrap für §14a EnWG Steuerboxen und stellt dafür
Spezifikation, Referenz-Implementierung und Konformanz-Suite bereit. Die Vorlage ist
bewusst so gehalten, dass sie angepasst, übernommen und weiterentwickelt werden kann:
jeder Akteur darf sie nutzen, verändern und in eigenen Produkten einsetzen. Damit
sollen Diskussions- und Implementierungskosten sinken und ein gemeinsames,
herstellerübergreifendes Verständnis der Schnittstellen entstehen.

Die Vorlage wird **Open Source unter der MIT-Lizenz ohne Royalties** veröffentlicht
(siehe [`LICENSE`](LICENSE)). Die MIT-Lizenz erlaubt die Nutzung, Vervielfältigung,
Veränderung und Weitergabe — auch kommerziell — bei Angabe des Urheberrechtsvermerks;
es fallen keine Lizenzgebühren an.

Referenzpaket für MSB-Plattformen zur Implementierung der beiden mTLS-Endpunkte des
bilateralen Zero-Touch-Bootstraps für §14a EnWG Steuerboxen:

1. **Discovery** — `GET /v1/serials/{serial|vpid}` (200 found / 404 not found; Transportfehler sind clientseitig unknown)
2. **Provisioning** — `POST /v1/serials/{serial}/provisioning` (idempotent, mTLS transportiert den SHIP-Info-Datensatz)

## Inhalt

| Pfad | Inhalt |
|---|---|
| `spec/` | **Normativ:** OpenAPI 3.1 (`discovery.yaml`, `provisioning.yaml`), JSON-Schemas (`wellknown.schema.json`, `pairing_dataset.schema.json`), Beispiele |
| `conformance/` | **Abnahme:** black-box pytest-Suite — läuft gegen jede MSB-Implementierung, prüft mTLS-only, Tri-State, Anti-Enumeration, Idempotenz, Rate-Limits, box_evidence, Fleet-Binding, Error-Taxonomie |
| `docs/` | Deutsche Implementierungsdoku (mkdocs-material), ADR-0001 … ADR-0019 (bindende Entscheidungen) |
| `reference/` | Eigenständiger Rust-Stack: MSB, EMS-Backend, Well-known, Box-/HEMS-Mocks und E2E-Runner |
| `test/ca/`, `fixtures/`, `deploy/compose/` | Lokal erzeugte Test-PKI, synthetische Parser-Fixtures und optionaler Compose-Stack |

> **Beispiel-IANA-PEN:** `12345` — die Beispiele, Schemas und die Conformance-Suite
> verwenden diese Beispiel-PEN. Ihre echte IANA PEN konfigurieren Sie selbst
> (`WK_IANA_PEN` / `MSB_IANA_PEN` bzw. `CONF_IANA_PEN` in der Conformance-Suite).

## Schnellstart (Abnahme)

### Vollständiger lokaler Referenzlauf

Voraussetzungen: Rust/Cargo (aktueller Stable-Toolchain), Make, Bash und OpenSSL.
Für Konformanz und Dokumentation zusätzlich Python 3.10+.

```bash
make test e2e             # Rust-Tests + 16 öffentliche E2E-Szenarien, ohne Docker
python3 -m venv .venv
.venv/bin/pip install -r requirements-dev.txt
make PYTHON=.venv/bin/python conformance-local public-check spec-validate
make demo                 # HappyPath inkl. simulierter SHIP-TLS-Kopplung
```

Der Stack ist unabhängig von Unternehmensdiensten. Zertifikate und Schlüssel
werden frisch unter dem gitignorierten `test/ca/out/` erzeugt. Beobachtungen
der Steuerbox werden als synthetische TXT-Daten eingespeist; die LAN-Kopplung
ist ein TLS/SKI-Mock, kein vollständiger EEBUS-SHIP-Protokollstack.
Logs: `reference/target/e2e-logs/<Szenario>/`.

Optional: `make e2e-docker` führt dieselben Szenarien mit Docker Compose aus.
GitLab-Projekte können dieses Repository als GitHub-Submodul einbinden;
siehe [Entwicklung und Synchronisation](docs/entwicklung.md).

### Gegen eine eigene MSB-Implementierung

```bash
# Konformanz-Suite gegen die eigene Staging-Umgebung
cd conformance
pip install -r requirements.txt
export TARGET=https://ihre-staging-url
export CONF_CERT=/pfad/zum/client-cert.pem
export CONF_KEY=/pfad/zum/client-key.pem
export CONF_IANA_PEN=12345   # die IANA PEN, die Ihr MSB aus unserem Well-known-Dokument gepinnt hat
export CONF_ROOT=/pfad/zum/server-root-bundle.pem
pytest tests/ -v --target="$TARGET"
```

Alle Tests müssen grün sein — die Suite ist das Abnahmekriterium.

## Normative Grundregeln (Kurzfassung)

- **mTLS-only** auf beiden Endpunkten — keine anonyme Mode, keine Bypass-Flags.
- **mTLS terminiert in der Anwendung** (k8s: TLSRoute / ssl-passthrough), nicht am Edge-Proxy (ADR-0009).
- **Tri-State:** Transportfehler ist clientseitig `unknown`, nie `found:false` (ADR-0005).
- **Tolerantes Matching:** exakte Regel-S-Basis, suffixlos erlaubt; kürzere Query als Portfolio-Serial → `found:false` (Anti-Enumeration, ADR-0004/-0012).
- **Write-once:** Serial↔MSB ändert sich nie (ADR-0006).
- **Idempotenz:** Provisioning auf `(serial, ship_id)` (ADR-0006).
- **box_evidence:** optional in der Payload; bei Mismatch → `422 box_mismatch` + Operator-Alarm (ADR-0007).
- **Secret-Transport:** der SHIP-Info-Datensatz inkl. SPSEC reist ausschließlich via mTLS; nie in Logs, Traces oder Fehlerkörpern (ADR-0008/-0017).
- **Rate-Limits sind Vertrag:** `429` + `Retry-After`, Clients honorieren (ADR-0010).
- **Well-known:** `/.well-known/steuerbox-connect/v1` — MSB pinnt die dedizierte Client-CA via TOFU + TTL (ADR-0003/-0013) und daraus die `iana_pen` des Herstellers (ADR-0018).
- **Fleet-Binding:** die IANA PEN in `pairing_dataset.ship_id` (`i:<PEN>_u:<VPID>`) MUSS der im Well-known-Dokument veröffentlichten `iana_pen` entsprechen — Fremd-Flotten-Daten sind `422 fleet_mismatch` + Operator-Alarm, nie ein stiller Ignore (ADR-0018).

Vollständige Begründungen: `docs/adr/`.

## Optionale Konfiguration pro Umgebung (Qualification-Handshake, ADR-0002)

Jede Umgebung (Staging / Produktion) hat **eigene** Endpunkte — der Austausch
erfolgt einmals bilateral pro Paar (Hersteller ↔ MSB), verankert an der IANA
PEN des Herstellers (ADR-0018) und out-of-band verifiziert. Kein zentrales Registry,
keine bilateralen Credential-Stores.

| Rolle | Stellt bereit |
|---|---|
| **Well-known des EMS-Herstellers** | Der Hersteller stellt den `/.well-known/steuerbox-connect/v1`-Endpunkt bereit und veröffentlicht darin seine dedizierte Client-CA (pinned by the MSB via TOFU + TTL, ADR-0003/-0013). |
| **MSB-API** | Der MSB stellt seine API-Basis-URL und zwei mTLS-Endpunkte bereit: die Serial-/ID-Abfrage (`GET /v1/serials/{serial|vpid}`) und die Übermittlung der EEBUS-Daten (`POST /v1/serials/{serial}/provisioning`). |

Beispielhafte Umgebungstabelle (pro Umgebung / pro Paar):

| Umgebung | Well-known URI (EMS-Hersteller) | MSB API Basis-URL |
|---|---|---|
| Staging | `https://ems-staging.example/.well-known/steuerbox-connect/v1` | `https://msb-staging.example` |
| Produktion | `https://ems.example/.well-known/steuerbox-connect/v1` | `https://msb.example` |

Zusätzlich vereinbart werden pro Paar: die beiden Endpunkte und Quotas
(Rate-Limits, ADR-0010). Endpoint-Änderungen und Rotation laufen über den
Ops-Contract (Anzeigepflicht), nicht über einen API-Endpunkt.

## Doku bauen

```bash
pip install -r docs/requirements.txt  # falls vorhanden; sonst: pip install mkdocs-material mkdocs-mermaid2-plugin
mkdocs serve   # aus dem Repo-Root
```

## Rechtliches

MIT-Lizenz, ohne Royalties (siehe [`LICENSE`](LICENSE)). Die Referenz-Implementierung
ist quelloffen; die Konformanz-Suite darf frei für die eigene Abnahme eingesetzt und
erweitert werden. Nutzung, Veränderung und kommerzieller Einsatz sind unter Wahrung
des Urheberrechtsvermerks gestattet — es fallen keine Lizenzgebühren an.
