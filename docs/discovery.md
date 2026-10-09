# Discovery & Matching

Normative Grundlage: `spec/discovery.yaml`.
Diese Seite vertieft die Matching-Semantik, die Serial-Ableitung und die
Enumeration-Schutzregeln. Referenz-Implementierung: `src/msb/` (Portfolio-Store,
toleranter Matcher), Parser-Golden-Records: `fixtures/golden/`.

## Zwei Serials, nie vermischt

| Kontext | Serial | Beispiel |
|---|---|---|
| Pfad/Query der MSB-API | **Steuerbox-Serial** | `GTHE0100045088` |
| `pairing_dataset.serial` | **HEMS-Serial** | `1U0022_CO-4206` |

Beide dürfen nie vermischt werden. Die Provisionierung schreibt **ausschließlich** auf
die kanonische Serial — mit Status-Code-Discovery (200/404, leerer Body) leitet der
Client sie deterministisch selbst aus der Query ab (Regel-S-Basis).

## Regel S — deterministische Serial-Ableitung

Die Steuerbox annonciert sich per mDNS (`_ship._tcp`). Real beobachtete TXT-Records
haben einen wertlosen `serial`-Key; die Serial steckt im `id`-Feld als VPID:

```text
id=i:51593_u:GTHE0100045088        → Serial: GTHE0100045088
id=i:51593_u:GTHE0100045088-KW47   → Serial: GTHE0100045088 (Suffix -KWW entfernt)
```

Ableitung: Split am **ersten** `-` (Suffix optional; ohne Hyphen → ganzer String).
Kanonische Serials matchen `^[A-Za-z0-9]{4,32}$` — keine Hyphens. Die
herstellerübergreifende ID im Feld `i:<IANA-PEN>_u:<VPID>` ist der Query-Schlüssel für
die Discovery.

## Tolerantes Matching

Der Matcher ist tolerant, aber **präfixlos** — Toleranz bezieht sich auf die Form der
Query, nicht auf Serial-Kürze:

1. **Regel-S-Basis:** Query und Portfolio-Serial werden beide auf ihre kanonische
   Regel-S-Basis reduziert (Split am ersten `-`; ohne Bindestrich → ganze Zeichenkette).
   Nur diese Basis wird verglichen.
2. **`-`-Delimiter:** Eine Query mit `-Suffix` matcht dieselbe Portfolio-Serial;
   die kanonische Serial (ohne Bindestrich, Regel-S-Basis der Query) ist das einzige
   gültige Write-Ziel.
3. **Exakte Basis-Gleichheit:** eine Query, deren Basis **kürzer** als die
   Portfolio-Serial ist, matcht **nie** (Anti-Enumeration, siehe unten). Ein
   „längster Präfix"-Match ist ausdrücklich **nicht** Teil des Protokolls — er würde
   die Anti-Enumerations-Regel aushebeln.

## Anti-Enumeration

Die Serial-Query darf nicht als Portfolioscan missbraucht werden können:

- **Query kürzer als die Portfolio-Serial → `404`.** Präfix-Anfragen liefern
  niemals `200` — auch wenn der Präfix zu Einträgen passt.
- Damit kostet das Systematisieren einer Portfolio-Serial-Räume pro Query ein
  echtes Volltreffer-Ereignis; Ratenlimits (`429` + `Retry-After`) engen das weiter ein.
- Normativ getestet durch das Szenario `AntiEnumeration` (e2e) und Unit-Tests gegen
  Golden Records.

## Tri-State

| Fall | Antwort |
|---|---|
| Serial im Portfolio | `200` (leerer Body) |
| Query nicht im Portfolio (vollständig, nicht kürzer), gekillt oder kürzer | `404` (leerer Body) |
| Transport-/TLS-/Verfügbarkeitsfehler, andere Statuscodes | `unknown` — **nie** `404` |

`unknown` existiert **nur clientseitig** — der Server überträgt es nie; er antwortet
ausschließlich mit `200`/`404` (spec/discovery.yaml, ADR-0005). Das Client-Backend
verweigert das Zero-Touch-Provisioning auf `unknown`. `429` ist auf Discovery ebenfalls
Kontrakt (`Retry-After`-Header obligatorisch).

## Testbarkeit

- **Golden Records** (`fixtures/golden/`, maskiert nach `SNTEST*`) sind die Wahrheit für
  die Parser-Korrektheit — e2e-Netzwerkkorrektheit schwächt **nie** die Parser-Tests.
- **E2E** zeigt die Verkabelung (mDNS → Parser → Matcher → Discovery).
- Parser-Änderungen ohne neuen Golden-Record-Fall und Match-Änderungen ohne
  Conformance-Case sind nicht mergebar.