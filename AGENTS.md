# Public reference project

- Work in this repository; the private parent project is a separate consumer.
- `spec/` and `docs/adr/` define the shared protocol. Keep public E2E and
  conformance scenarios intact when integrating upstream changes.
- Rust components live in `reference/`; `make test e2e conformance-local` must
  work without a private checkout, credentials or company services.
- Public test identities are synthetic (`SNTEST*`, example PEN `12345`).
  Generate keys only in gitignored `test/ca/out/`; never import inventory,
  production QR data, environment files or private deployment configuration.
- Use `make public-check` before publishing. Scan findings are reported as
  filenames and line numbers, without echoing matched values.
- The private consumer embeds this repository as a Git submodule. Shared
  changes are committed in this repository (or its submodule checkout), then
  the consumer advances its gitlink. Do not copy the private repository back
  wholesale. See `docs/entwicklung.md` for both directions.
- German operator documentation; English identifiers and test names.
