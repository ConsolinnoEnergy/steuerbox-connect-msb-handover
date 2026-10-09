# Sicherheit & Trust

## Das Zwei-CA-Modell

Zwei unabhängige CA-Domänen, niemals gemischt, niemals gekoppelte Lebenszyklen:

| | Server-TLS (MSB-API + Well-known) | Client-mTLS (EMS-Backend → MSB) |
|---|---|---|
| Produktion (k8s) | Let's Encrypt via cert-manager | eigene, dedizierte Sub-CA des EMS-Herstellers |
| Lokal (Compose) | Test-Root-CA „sim-LE" | Test-Client-CA (gleicher Code-Pfad) |
| Vertraut durch | konfigurierbares Root-Bundle (`TLS_ROOT_BUNDLE`) | MSB-Pin aus Well-known (TOFU + TTL + Grace) |
| Rotation | automatisch (ACME) | autonom; Well-known veröffentlicht Grace-Fenster |

**Client-Authentifizierung:** Das EMS-Backend nutzt ein Client-Cert, das von der CA des
EMS-Herstellers signiert wurde. Der MSB prüft das Client-Cert gegen die CAs in seinem
Pin-Store; die CA veröffentlicht der Hersteller per Well-known-Dokument (base64-PEM
plus Gültigkeitsfenster, ADR-0013).

Invarianten:

- Der Anwendungscode verzweigt **nie** auf die ausstellende CA; Unterschiede leben
  ausschließlich in Konfiguration und Manifesten.
- Die Client-Sub-CA ist **dediziert** (`scope: steuerbox-connect-api`).
- mTLS terminiert **in der Anwendung**, nie am Edge-Proxy.
- Kein Key-Material im Git — CI erzwingt es (`gitleaks`, `test/ca/out/` ist gitignored).
- Crypto: nur RustCrypto-Ökosystem (`rustls`, `rcgen`, `ring`, …), keine OpenSSL-Bindings.

## Hard Rules (PR-blockierend)

- mTLS-only auf beiden MSB-Endpunkten; kein anonymer Modus, keine Bypass-Flags.
- Tri-State: Transportfehler ist `unknown`, nie `404`.
- **Kein SHIP-Secret** in Payloads, Logs, Fixtures oder Beispielen — nur der Admin-Kanal.
- Write-once: Serial↔MSB nie ändernd.
- Provisioning schreibt nur auf kanonischer Serial.
- `box_evidence`-Mismatch → `422 box_mismatch` + Alarm, nie stilles Ignorieren.
- Rate Limits sind Vertrag: `429` + `Retry-After`; Clients halten sie ein.
- Provisioning-Bodies werden **nie geloggt** — nur Metadaten.
- LAN-Claims (spoofed mDNS) kosten höchstens einen Lookup, werden nie eine Kopplung.

## Threat-Modell (Kurzfassung)

| Bedrohung | Gegenmaßnahme |
|---|---|
| Rogue-Client-Zertifikat | Pinning der Hersteller-CA (TOFU + TTL), Kill-Switch, Szenario `RogueClientCA` |
| Client-CA-Rotation-Angriff | Grace-Fenster im Well-known, Re-Fetch, Szenario `ClientCARotation` |
| Anti-Enumeration | kurze Query → `404`; Szenario `AntiEnumeration` |
| Spoofed mDNS im LAN | Closed Loop: nur akzeptierter SHIP-Handshake koppelt; Szenario `NetworkBoundaries` |
| Secret-Leak | kein Secret in der API; gitleaks-Gate; Szenario `NetworkBoundaries` |
| Box-Evidence-Spoofing | Inventar-Abgleich, `422 box_mismatch` + Operator-Alarm; Szenario `BoxEvidenceMismatch` |

Vollständige Ausarbeitung: [threat-model.md](threat-model.md) · Enrollment-Vertiefung:
[enrollment.md](enrollment.md) · Matching-Vertiefung: [discovery.md](discovery.md) ·
bindende Entscheidungen: [ADRs](adr/index.md).