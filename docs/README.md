# docs

Quellen der GitLab-Pages-Dokumentation (mkdocs-material, `mkdocs.yml` im Repo-Root,
Pipeline-Job `pages` auf dem Default-Branch):

| Datei | Inhalt |
|---|---|
| `index.md` | Überblick, Scope (Backend-zu-Backend), Beteiligte, Schnellstart |
| `prozess.md` | Der bilaterale Ablauf in 10 Schritten (Sequenzdiagramm + Invarianten) |
| `rolle/ems-hersteller.md` | Implementierungspflichten EMS-Hersteller (Well-known, Client-CA, Backend, HEMS) |
| `rolle/msb.md` | Implementierungspflichten MSB (zwei mTLS-Endpunkte, Pinning, Parameterisierung, §12) |
| `spec/index.md` | Spec-Referenz, Matching-Regeln, offene Spec-Lücken |
| `security.md` | Zwei-CA-Modell, Hard Rules, Threat-Modell-Kurzfassung (Vertiefung: `threat-model.md`, `discovery.md`, `enrollment.md`) |
| `testen.md` | Testebenen, e2e-Szenarien, Conformance für eigene Implementierungen |
| `glossar.md` | bindende Begriffe |
| `adr/index.md` | ADR-Index (0001–0011, alle Accepted) |

Ausstehend (geplant, noch nicht geschrieben):

- `adr/0001…0011` — pending

Sprachregelung: Diese Doku ist **deutsch** (Zielgruppen: deutsche MSB-Plattformen und
EMS-Hersteller). Code-Identifikatoren, ADRs, Commits und Testnamen bleiben englisch
(s. `AGENTS.md` §Language policy).

Befehle: `make docs` (Build, strict), `make docs-serve` (Lokalserver).