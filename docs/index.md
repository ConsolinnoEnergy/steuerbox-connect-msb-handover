# eebus-pairing

**Offene Referenzimplementierung des bilateralen Zero-Touch-Bootstraps zwischen
EMS-Herstellern und Messstellenbetreibern (MSB) für Steuerboxen (§14a EnWG).**

> **Status:** Referenzprojekt / POC. Spezifikation und Abnahmetests definieren den
> Schnittstellenvertrag; LAN-Geräte sind simuliert. **Lizenz:** MIT.
> Diese Seite ist die Implementierungs-Dokumentation für EMS-Hersteller und
> Messstellenbetreiber (MSB).

## Scope

Dieses Repo bildet **ausschließlich die Backend-zu-Backend-Schiene** ab:

| Bestandteil | Im Repo? |
|---|---|
| Discovery- und Provisioning-API (mTLS) | ✅ Spec (`spec/`) + Referenz-MSB (`reference/msb/`, Library `reference/msb-core/`) |
| Well-known-Endpunkt & Client-CA-Pinning | ✅ Spec + Referenz (`reference/wellknown/`, `reference/ems-backend/`) |
| Referenz-EMS-Backend (Client) | ✅ `reference/ems-backend/` |
| E2E-/Conformance-Suite, POC, Doku | ✅ `reference/e2e/`, `conformance/`, diese Seite |
| HEMS-Installation beim Kunden | ❌ (nur Prozesskontext) |
| LAN-Erkennung der Steuerbox im Feld | ❌ (im Repo nur als HEMS-Mock-Eingabe) |
| SMGW-Installation & -Parameterisierung | ❌ (nur Prozesskontext) |
| §12-EnWG-Steuerungsnachweis | ❌ (nur Prozesskontext) |

## Beteiligte

- **EMS-Hersteller** — baut das EEBUS-zertifizierte HEMS, betreibt das EMS-Backend,
  veröffentlicht den Well-known-Endpunkt mit seiner Client-CA.
- **Messstellenbetreiber (MSB)** — betreibt SMGW und Steuerbox, stellt die beiden
  mTLS-API-Endpunkte bereit, parametriert die Steuerbox, führt den Steuerungsnachweis.
- **HEMS (EMS-Gerät)** — erkennt die Steuerbox im Kundennetzwerk und meldet die
  EEBUS-Daten an das EMS-Backend.
- **SMGW / Steuerbox** — Smart-Meter-Gateway und §14a-Steuereinheit im Feld.

## Startpunkte je Rolle

- Sie bauen ein HEMS bzw. ein EMS-Backend? → [Für EMS-Hersteller](rolle/ems-hersteller.md)
- Sie betreiben Steuerboxen als MSB? → [Für Messstellenbetreiber](rolle/msb.md)
- Sie wollen den Ablauf verstehen? → [Prozess (10 Schritte)](prozess.md)
- Sie implementieren die API? → [Spezifikation](spec/index.md)

## Schnellstart (Referenz-Stack lokal)

```bash
git clone https://github.com/ConsolinnoEnergy/steuerbox-connect-msb-handover.git
cd steuerbox-connect-msb-handover
make e2e        # frische CA erzeugen, alles bauen, 16 Szenarien als lokale Prozesse
make demo       # nur HappyPath_ZeroTouch
```

Für Konformanz, Python-Abhängigkeiten und Submodul-Entwicklung siehe
[Testen](testen.md) und [Entwicklung](entwicklung.md).

Status: Prototyp/POC. Lizenz MIT, keine Rechtsberatung.
