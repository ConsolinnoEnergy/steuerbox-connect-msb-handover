# Glossar

| Begriff | Bedeutung |
|---|---|
| MSB / Messstellenbetreiber | Betreiber der Steuerbox; Plattform / Tenant als Anmeldeeinheit |
| Steuerbox | Steuereinheit je §14a EnWG; **kein Typenschild** — nie auf ein Geräte-Label referenzieren |
| HEMS | EEBUS-zertifiziertes Heimenergiemanagementsystem des EMS-Herstellers |
| SHIP / SHIP-ID / SKI / FPH | LAN-Kopplungsprotokoll, Geräte-ID, Subject Key Identifier, Fingerprint (FNN-Credential-Set) |
| VPID | Vendor-Teil von `i:<IANA-PEN>_u:<VPID>`; heute: Serial + optionaler `-Suffix` |
| Herstellerübergreifende ID | die ID in der Box-SHIP-ID (`i:<IANA-PEN>_u:<VPID>`); der Discovery-Query-Schlüssel |
| Gerätee-Token | Device-Token-Auth des HEMS-Geräts gegenüber dem EMS-Backend (Verfahren des Herstellers, z.B. JWT — außerhalb dieses Repos) |
| SMGW | Smart-Meter-Gateway des MSB; trägt den Admin-Pfad zur Steuerbox |
| TAF | Technische Anschlussbedingung-Variante an SMGWv2 zur Box-Parameterisierung |
| pEMT | physikalischer EM-Treiber; Auswertungsseite des §12-EnWG-Steuerungsnachweises |
| Regel S | deterministische Serial-Ableitung: Split am ersten `-`; kanonische Serials ohne Hyphen |
| Kanonische Serial | die Portfolio-Serial aus der Discovery-Antwort; das einzige gültige Write-Ziel |
| box_evidence | im LAN beobachtete Steuerbox-ship_id + SKI, mit der Provisionierung gesendet (Mismatch → 422) |
| Well-known | `/.well-known/steuerbox-connect/v1`: `iana_pen` (IANA PEN des Herstellers, z.B. `12345`, ADR-0018), `client_ca`, `contact` |
| Sim-LE | lokale Test-Server-CA als Let's-Encrypt-Stellvertreter (LE-artige Kette, kurze Laufzeiten) |
| Client-CA | dedizierte Sub-CA des EMS-Herstellers für mTLS-Client-Certs; öffentlicher Teil wird vom MSB gepinnt |
| Closed Loop | Provisioning gilt erst durch einen akzeptierten SHIP-Handshake im LAN als bewiesen |
| Tri-State | `200` found / `404` not found / `unknown` (nur clientseitig) |
| Qualifikation | einmaliger bilateraler Austausch: Well-known-URI ↔ MSB-API-URLs + Quoten, verankert an der IANA PEN (ADR-0018) |
| Ops-Vertrag | vertragliche Bausteine zwischen MSB und Dienstleister (Informationspflichten, Tenant-Policy, Quoten) |