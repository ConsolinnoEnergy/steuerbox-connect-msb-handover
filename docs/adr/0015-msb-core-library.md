# ADR-0015: msb-core als einbindbare Referenz-Library (Store-Trait-Integration)

- **Status:** Accepted
- **Datum:** 2026-09-25
- **Kontext:** MSB-Rolle, Einbau in bestehende MSB-Backends

## Kontext

Die Referenz-MSB (`msb`) war ein reines Binary mit hart verdrahtetem In-Memory-Store.
Ein echter Messtellenbetreiber konnte die normative Logik (tolerantes Matching,
Tri-State, Write-once/Idempotenz, Kill-Switch, Rate-Limit, mTLS-Terminierung, Fehlertaxonomie)
nicht wiederverwenden — er hätte alles nachimplementieren und über die Conformance-Suite
bringen müssen. Das Repo-Mission („Referenz-Implementierung", AGENTS.md §1) verlangt aber
genau das: ein MSB bindet die Referenz in sein Backend ein und steckt nur noch seine echte
Portfolio-/Inventar-DB an.

## Entscheidung

1. **Neue Library-Crate `msb-core`** (`src/msb-core/`): Matcher, Store-Verträge, Handler,
   axum-Router, mTLS-Serve-Loop, Box-Admin-Adapter-Trait. Kein Binary, keine Env-Parsing-Logik.
2. **Integrationsnaht ist der `MsbStore`-Trait** (5 Methoden):
   - `lookup(query)` — tolerant (VPID-Form, `-suffix`), Anti-Enumeration,
   - `is_killed(serial)` / `kill(serial)` — Kill-Switch,
   - `in_portfolio(serial)` — Portfolio-Zugehörigkeit (kein Existenz-Leak),
   - `inventory(serial)` — erwartete Box-Evidence (ADR-0007),
   - `provision(serial, record)` — idempotent auf `(serial, ship_id)`, write-once (ADR-0006).
   Die Semantik jedes Methode ist **normativ** (AGENTS.md §6, ADR-0004/-0005/-0006/-0007/-0012)
   und wird black-box durch die Conformance-Suite geprüft — eine eigene Implementierung MUSS
   sie einhalten.
3. **`InMemoryStore` bleibt der Referenz-Dummy**: nutzbar für Tests, Compose/e2e und als
   Template; der `msb`-Binary wird ein dünner Wrapper (`MsbState::new + with_tls + serve`).
4. **Rate-Limiting lebt in Core** (`RateLimiter`), nicht im Store: ein einbindender MSB kann
   seinen eigenen Limiter nutzen, aber das beobachtbare Verhalten (429 + truthful
   `Retry-After`, ADR-0010) ist kontraktuell.
5. **TLS-Material ist optional**: `MsbState::new` braucht keine Zertifikatspfade; ein MSB,
   das `api_router` hinter seinen eigenen mTLS-Server mountet, konfiguriert TLS selbst
   (Terminierung bleibt in der Anwendung, ADR-0009). `serve()`/`serve_tls` bauen die
   mTLS-Konfiguration lazy beim ersten Verbindungsaufbau (Rotation-Grace unverändert).

## Konsequenzen

- Verhalten des Compose-Stacks und der e2e-Szenarien ist unverändert (gleiche Codepfade,
  nur andere Crate-Grenze). Kein e2e-Scenario ändert sich.
- Neue Verhaltensregel: MSB-Backends haben eine dokumentierte, testgetriebene
  Einbaustrecke; Doku in `docs/rolle/msb.md` (Abschnitt „Referenz-Implementierung einbinden").
- `msb-core` ist Teil des Workspace-Lint/Audit-Gates; Dependency-Änderungen folgen §5
  (cargo audit/deny).
- Kill-Switch über den Admin-Kanal geht durch denselben Trait — auch echte DBs werden
  damit beschreibbar; der Admin-Kanal bleibt token-geschützt und separater Bind (AGENTS §6).

## Verwandte Entscheidungen

- ADR-0004/-0012 (tolerantes, präfixloses Matching) — übernommen vom Matcher in Core.
- ADR-0006/-0007 (Idempotenz/Write-once, box_evidence) — Vertrag im Trait dokumentiert.
- ADR-0009 (mTLS in der Anwendung) — `serve_tls` ist optional; Router-Mount erlaubt.