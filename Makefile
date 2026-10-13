# Public reference stack. All commands run from this repository, without private services.
RUST ?= cargo
PYTHON ?= python3
TARGET ?= https://localhost:18443

.PHONY: help test-ca build test e2e e2e-docker demo conformance conformance-local spec-validate lint lint-rust lint-python public-check docs docs-serve up down clean
help:
	@awk 'BEGIN{FS=":"} /^[a-z][a-z-]*:/ {print "  make " $$1}' Makefile

test-ca:
	$(MAKE) -C test/ca ca-all
build:
	$(RUST) build --manifest-path reference/Cargo.toml --workspace --locked
test:
	$(RUST) test --manifest-path reference/Cargo.toml --workspace --locked
e2e: test-ca build
	$(RUST) run --manifest-path reference/Cargo.toml --locked -p e2e
e2e-docker: test-ca build
	E2E_STACK=docker $(RUST) run --manifest-path reference/Cargo.toml --locked -p e2e
demo: test-ca build
	E2E_SCENARIO=HappyPath_ZeroTouch $(RUST) run --manifest-path reference/Cargo.toml --locked -p e2e
conformance:
	$(PYTHON) -m pytest conformance/tests -v --target="$(TARGET)"
conformance-local: test-ca build
	PYTHON="$(PYTHON)" E2E_SCENARIO=Conformance $(RUST) run --manifest-path reference/Cargo.toml --locked -p e2e
spec-validate:
	$(PYTHON) tools/validate_spec.py
	$(PYTHON) tools/build_openapi.py --check
spec-docs:
	$(PYTHON) tools/build_openapi.py
coverage:
	bash tools/coverage.sh
lint: lint-rust lint-python
lint-rust:
	$(RUST) fmt --manifest-path reference/Cargo.toml --all -- --check
	$(RUST) clippy --manifest-path reference/Cargo.toml --workspace --all-targets --locked -- -D warnings
lint-python:
	$(PYTHON) -m ruff check conformance tools
public-check:
	$(PYTHON) tools/check_public.py
docs:
	$(PYTHON) -m mkdocs build --strict
docs-serve:
	$(PYTHON) -m mkdocs serve
up: test-ca
	docker compose -f deploy/compose/docker-compose.yml up -d --build
down:
	docker compose -f deploy/compose/docker-compose.yml down
clean:
	$(MAKE) -C test/ca ca-clean
	$(RUST) clean --manifest-path reference/Cargo.toml
