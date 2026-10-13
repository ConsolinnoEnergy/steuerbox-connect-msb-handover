# Entwicklung mit GitHub-Submodul

Das öffentliche GitHub-Repository ist die Quelle für Spezifikation, gemeinsame
Rust-Komponenten, Mock-Dienste und öffentliche Abnahmetests. Das private
GitLab-Projekt bindet einen **konkreten Commit** als Submodul unter
`public/steuerbox-connect/` ein. Private Integrationen bleiben im Elternprojekt.

## Erstmalig einbinden

Im GitLab-Projekt:

```bash
git submodule add --name steuerbox-connect \
  https://github.com/ConsolinnoEnergy/steuerbox-connect-msb-handover.git \
  public/steuerbox-connect
git -C public/steuerbox-connect switch main
git add .gitmodules public/steuerbox-connect
```

Der öffentliche Stand muss vor der Aktualisierung des GitLab-gitlinks auf
GitHub verfügbar sein. Ein gitlink speichert keine uncommitted Dateien.

Für neue Checkouts: `git clone --recurse-submodules <gitlab-repository>`.
GitLab CI setzt `GIT_SUBMODULE_STRATEGY: recursive` und verwendet denselben
gepinnten Commit wie lokale Checkouts.

## GitHub → GitLab

```bash
bash public/steuerbox-connect/tools/submodule.sh update
make test e2e
git add public/steuerbox-connect
git commit -m "chore: update public reference submodule"
```

`update` verwendet Fast-forward und führt die öffentliche Abnahme durch.
Es aktualisiert keine internen Dateien und committed den gitlink nicht automatisch.

## GitLab → GitHub

Gemeinsamen Code direkt **im Submodul** bearbeiten:

```bash
git -C public/steuerbox-connect switch -c fix/shared-protocol
# Änderungen innerhalb public/steuerbox-connect/ vornehmen
bash public/steuerbox-connect/tools/submodule.sh verify
git -C public/steuerbox-connect add <gemeinsame-dateien>
git -C public/steuerbox-connect commit -m "fix: shared protocol behavior"
git -C public/steuerbox-connect push -u origin fix/shared-protocol
# GitHub-PR zusammenführen; danach im Elternprojekt den gitlink aktualisieren
```

Ein separater GitHub-Checkout und der GitLab-Submodul-Checkout sind normale
Git-Checkouts desselben Repositories. Synchronisation erfolgt über Branches,
Fetch/Pull und PRs. Private Commits des GitLab-Elternprojekts werden nicht ins
öffentliche Repository übertragen.

## Build-Anbindung

Private Crates referenzieren öffentliche Libraries als Pfadabhängigkeiten,
beispielsweise `../../public/steuerbox-connect/reference/ems-backend`.
Das öffentliche Cargo-Workspace bleibt eigenständig. Öffentliche Binaries und
Tests werden über `make -C public/steuerbox-connect build test e2e` gebaut.
Damit werden gemeinsame Komponenten nicht als zweite Kopie gepflegt.

Der interne Runner bindet zusätzlich die öffentliche `e2e`-Library ein und
ergänzt nur seinen privaten Inventory-Test. Die internen `spec/`, `docs/`,
`conformance/` und `fixtures/`-Pfade sind direkte Verweise auf das Submodul.
Gemeinsame Ableitungs- und Matching-Regeln liegen einmalig in
`reference/protocol/`. Die Swagger-Ansicht wird mit `make spec-docs` aus
`spec/` generiert; `make spec-validate` erkennt eine veraltete Ansicht.

## Verifikation

```bash
python3 -m venv .venv
.venv/bin/pip install -r requirements-dev.txt
make PYTHON=.venv/bin/python public-check spec-validate test e2e conformance-local
```

Optionaler Abdeckungsreport:

```bash
cargo install cargo-llvm-cov --locked
rustup component add llvm-tools-preview
make coverage
```

E2E und Konformanz nutzen dieselben lokalen Ports und werden nacheinander
ausgeführt. `make e2e` läuft ohne Docker; `make e2e-docker` prüft dieselben
16 öffentlichen Szenarien mit Compose.
