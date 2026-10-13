#!/usr/bin/env bash
# Unit + process E2E coverage; services terminate normally and flush their profiles.
set -euo pipefail
ROOT=$(cd "$(dirname "$0")/.." && pwd)
cd "$ROOT"
export CARGO_TARGET_DIR="$ROOT/reference/target/llvm-cov-target"
cargo clean --manifest-path reference/Cargo.toml --target-dir "$CARGO_TARGET_DIR"
eval "$(cargo llvm-cov show-env --manifest-path reference/Cargo.toml --no-rustc-wrapper --sh)"
export LLVM_PROFILE_FILE="$CARGO_LLVM_COV_TARGET_DIR/%m-%p.profraw"
mkdir -p "$CARGO_LLVM_COV_TARGET_DIR"
find "$CARGO_LLVM_COV_TARGET_DIR" -maxdepth 1 -name '*.profraw' -delete
cargo build --manifest-path reference/Cargo.toml --workspace --locked
cargo test --manifest-path reference/Cargo.toml --workspace --locked
make test-ca
E2E_BIN_DIR="$CARGO_LLVM_COV_TARGET_DIR/debug" \
  "$CARGO_LLVM_COV_TARGET_DIR/debug/e2e-runner"
cargo llvm-cov report --manifest-path reference/Cargo.toml --json \
  --output-path reference/target/coverage.json
cargo llvm-cov report --manifest-path reference/Cargo.toml
