# Threat-Modell

Kurzfassung des Trust- und Angriffsmodells für den bilateralen Zero-Touch-Bootstrap.
Die normativen Gegenmaßnahmen sind als e2e-Szenarien verdrahtet
(s. [Testen](testen.md)) — ein Threat ohneScenario ist eine Lücke, kein Status quo.

## Assets

| Asset | Schutzziel |
|---|---|
| Pairing-Dataset (SHIP-ID, SKI, QR-Datensatz inkl. SPSEC-Secret — Vertraulichkeit via mTLS, ADR-0017) | Vertraulichkeit, Integrität & Authentizität |
| SHIP-Secret / FNN-Credentials | **Konfidentialität — nur Admin-Kanal, nie die API** |
| Client-CA (privater Teil) | Beim Hersteller, niemals im Git / k8s-Git |
| Pin-Store des MSB | Integrität, sofortige Sperrbarkeit |
| Portfolio-Serials des MSB | Anti-Enumeration (kein Portfolioscan) |
| Steuerbox-Admin-Kanal | Nur der MSB schreibt (token-gated) |

## Akteure & Annahmen

- **EMS-Hersteller:** vertrauenswürdig nach Qualifikation; kann kompromittiert werden
  (Leaf-CA-Diebstahl).
- **MSB:** betreibt Portfolio, SMGW, Steuerbox; Plattform-/Tenant-Topologie klein
  (~4 Dienstleister × ~6 Hersteller).
- **Angreifer im Kunden-LAN:** kann mDNS spoofen, kann keine MSB-API-Koordinaten
  einsehen (HEMS-Evidence leakt keine).
- **Angreifer im Internet:** kann Portfolios raten (Enumeration), Rogue-CAs betreiben,
  Endpunkte spoofen.

## Bedrohungen & Gegenmaßnahmen

| # | Bedrohung | Gegenmaßnahme | Normatives Szenario |
|---|---|---|---|
| T1 | Rogue-Client-Zertifikat / fremde CA | CA-Pinning (TOFU + TTL), TLS schlägt fehl | `RogueClientCA` |
| T2 | Kompromittierte/gepöbelte Client-CA | Sofortige Entfernung aus dem MSB-Pin-Store, kein Grace für Entfernung; Serial-Kill-Switch sofort wirksam | `KillSwitch` |
| T3 | CA-/Leaf-Rotation als Fensterangriff | Grace-Fenster im Well-known, Pin-Store-Reload | `ClientCARotation` |
| T4 | Portfoliorexploration über Serial-Queries | Anti-Enumeration: kürzere Query → `false` | `AntiEnumeration` |
| T5 | Spoofed mDNS im LAN (falsche Box/Serial) | Serial-Spoof kostet höchstens einen Lookup — Kopplung entsteht nie ohne akzeptierten SHIP-Handshake (Closed Loop) | `HappyPath_ZeroTouch` |
| T6 | Box-Evidence-Spoofing / falsche SKI | Inventar-Abgleich → `422 box_mismatch` + Operator-Alarm, nie still | `BoxEvidenceMismatch` |
| T7 | Secret-Leak über API/Payload/Logs | SHIP-Secret reist nur im Admin-Kanal; Log-Policy (nur Metadaten); Secret-Scan | `NetworkBoundaries` |
| T8 | Box-Admin-Write durch Dritte | Admin nur mit Token, nur aus dem MSB-Netz reachbar | `NetworkBoundaries` |
| T9 | MSB down → falscher Zustand | Tri-State: Transportfehler = `unknown`, Provisioning verweigert auf Unknown | `TriState_Unknown` |
| T10 | Doppel-Write/Replay | Idempotenz auf `(serial, ship_id)`, Write-once | `IdempotentPost` |
| T11 | Abuse/DoS über die API | Rate Limits als Vertrag: `429` + `Retry-After` | `RateLimit` |
| T12 | Endkunden-Transparenz fehlt | MSB-Portal-Meldung bei Kopplung — billigster Kompromittierungs-Indikator | Prozess (Ops-Vertrag) |

## TOFU-Restrisiko

TOFU-Pinning vertraut dem **ersten** Well-known-Fetch. Gegenmaßnahmen gegen den
First-Fetch-Angriff: Qualifikations-Handshake außer Band (Hersteller teilt URI
persönlich/verifiziert mit), IANA PEN als Flotten-Anker (ADR-0018), TTL + Re-Fetch,
Kill-Switch. Verbleibendes Risiko wird im Ops-Vertrag (Informationspflicht bei
Endpunktwechsel) getragen — ein Austausch über Kanäle außerhalb dieses Systems ist
Teil des Prozesses, nicht der Software.