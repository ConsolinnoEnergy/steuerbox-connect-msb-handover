#!/usr/bin/env bash
# Run from the private consumer's root; the public checkout remains an ordinary Git repo.
set -euo pipefail
URL=https://github.com/ConsolinnoEnergy/steuerbox-connect-msb-handover.git
MODULE=${PUBLIC_SUBMODULE_PATH:-public/steuerbox-connect}
case ${1:-status} in
  init)
    git submodule add --name steuerbox-connect "$URL" "$MODULE"
    git -C "$MODULE" switch main
    ;;
  update)
    git submodule update --init --recursive "$MODULE"
    git -C "$MODULE" diff --quiet
    git -C "$MODULE" diff --cached --quiet
    test -z "$(git -C "$MODULE" ls-files --others --exclude-standard)" || {
      echo "Submodule has untracked files; commit shared work before updating." >&2; exit 1;
    }
    git -C "$MODULE" switch main
    git -C "$MODULE" pull --ff-only origin main
    make -C "$MODULE" public-check test e2e conformance-local
    ;;
  verify)
    make -C "$MODULE" public-check spec-validate test e2e conformance-local
    ;;
  status)
    git submodule status "$MODULE"
    git -C "$MODULE" status --short
    ;;
  *) echo "Usage: bash tools/submodule.sh {init|update|verify|status}" >&2; exit 2 ;;
esac
