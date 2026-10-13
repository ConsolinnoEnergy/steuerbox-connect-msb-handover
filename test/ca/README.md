# Lokale Test-PKI

`make test-ca` erzeugt ausschließlich lokale Testzertifikate und QR-Testschlüssel.
Jeder Aufruf regeneriert `out/`, damit die 24-Stunden-Leaf-Zertifikate gültig sind
und CA-Rotationstests mit einem frischen Pin-Store starten.

- Server-TLS: sim-LE-Root → Intermediate → MSB / Well-known.
- Client-mTLS: separate Client-Root → dedizierte Client-Sub-CA → Client-Leaf.
- Zweite Client-CA und zweites Leaf für Rotation; Rogue-CA für Negativtests.
- RSA-Testschlüsselpaar für den verschlüsselten QR-Kanal im HEMS-Mock.

`out/` ist gitignoriert. Die Schlüssel werden lokal erzeugt; es werden keine
Zertifikate oder Zugangsdaten einer Unternehmensumgebung benötigt.
