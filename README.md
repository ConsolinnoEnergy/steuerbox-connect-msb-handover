# Steuerbox-Connect — MSB-Implementierungspaket

Referenzpaket für MSB-Plattformen zur Implementierung der beiden mTLS-Endpunkte des
bilateralen Zero-Touch-Bootstraps für §14a EnWG Steuerboxen:

1. **Discovery** — `GET /v1/serials/{serial|vpid}` (tristate: 200 found / 404 not found / 503 unknown)
2. **Provisioning** — `POST /v1/serials/{serial}/provisioning` (idempotent, mTLS transportiert den SHIP-Info-Datensatz)

## Inhalt

| Pfad | Inhalt |
|---|---|
| `spec/` | **Normativ:** OpenAPI 3.1 (`discovery.yaml`, `provisioning.yaml`), JSON-Schemas (`wellknown.schema.json`, `pairing_dataset.schema.json`), Beispiele |
| `conformance/` | **Abnahme:** black-box pytest-Suite — läuft gegen jede MSB-Implementierung, prüft mTLS-only, Tri-State, Anti-Enumeration, Idempotenz, Rate-Limits, box_evidence, Error-Taxonomie |
| `docs/` | Deutsche Implementierungsdoku (mkdocs-material), ADR-0001 … ADR-0017 (bindende Entscheidungen) |
| `reference/` | Optionaler Rust-Referenz-Code (`msb-core`, `msb`) — tolerant matcher, evidence verifier, pin store. **So kann es aussehen**, nicht als Kopiervorlage |

## Schnellstart (Abnahme)

```bash
# Konformanz-Suite gegen die eigene Staging-Umgebung
cd conformance
pip install -r requirements.txt
export CONF_BASE_URL=https://ihre-staging-url
export CONF_CERT=/pfad/zum/client-cert.pem
export CONF_KEY=/pfad/zum/client-key.pem
pytest tests/ -v
```

Alle Tests müssen grün sein — die Suite ist das Abnahmekriterium.

## Normative Grundregeln (Kurzfassung)

- **mTLS-only** auf beiden Endpunkten — keine anonyme Mode, keine Bypass-Flags.
- **mTLS terminiert in der Anwendung** (k8s: TLSRoute / ssl-passthrough), nicht am Edge-Proxy (ADR-0009).
- **Tri-State:** Transportfehler ist `unknown` (503), nie `found:false` (ADR-0005).
- **Tolerantes Matching:** exakte Regel-S-Basis, suffixlos erlaubt; kürzere Query als Portfolio-Serial → `found:false` (Anti-Enumeration, ADR-0004/-0012).
- **Write-once:** Serial↔MSB ändert sich nie (ADR-0006).
- **Idempotenz:** Provisioning auf `(serial, ship_id)` (ADR-0006).
- **box_evidence:** optional in der Payload; bei Mismatch → `422 box_mismatch` + Operator-Alarm (ADR-0007).
- **Secret-Transport:** der SHIP-Info-Datensatz inkl. SPSEC reist ausschließlich via mTLS; nie in Logs, Traces oder Fehlerkörpern (ADR-0008/-0017).
- **Rate-Limits sind Vertrag:** `429` + `Retry-After`, Clients honorieren (ADR-0010).
- **Well-known:** `/.well-known/steuerbox-connect/v1` — MSB pinnt die dedizierte Client-CA via TOFU + TTL (ADR-0003/-0013).

Vollständige Begründungen: `docs/adr/`.

## Doku bauen

```bash
pip install -r docs/requirements.txt  # falls vorhanden; sonst: pip install mkdocs-material mkdocs-mermaid2-plugin
mkdocs serve   # aus dem Repo-Root
```

## Rechtliches

Apache-2.0. Die Referenz-Implementierung ist quelloffen; die Konformanz-Suite darf frei
für die eigene Abnahme eingesetzt und erweitert werden.
