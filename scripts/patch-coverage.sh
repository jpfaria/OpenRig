#!/usr/bin/env bash
# patch-coverage.sh — the number codecov/patch will report, before you push.
#
# Codecov scores a PR on the lines the branch ADDS or CHANGES, not on the whole
# repo: instrumented added lines that no test executes are the whole verdict.
# This reproduces it locally from `cargo llvm-cov --lcov` + the branch diff, so
# a release PR never discovers a red patch check in CI again (#913).
#
# Usage:
#   ./scripts/patch-coverage.sh                  # against origin/develop
#   ./scripts/patch-coverage.sh release/v0.4.0   # against another base
#
# Env:
#   LCOV=path/to/lcov.info   reuse a report instead of re-running llvm-cov
#                            (the instrumented run rebuilds the workspace)
#   PATCH_TARGET=65          minimum % to exit 0 (default: codecov.yml's target)
#
# Exit: 0 = at or above target, 1 = below, 2 = could not measure.

set -euo pipefail

cd "$(dirname "$0")/.."

BASE="${1:-origin/develop}"

if ! git rev-parse --verify --quiet "$BASE" >/dev/null; then
  echo "patch-coverage: base '$BASE' does not exist — fetch it first" >&2
  exit 2
fi

LCOV_PATH="${LCOV:-}"
if [ -z "$LCOV_PATH" ]; then
  if ! command -v cargo-llvm-cov >/dev/null 2>&1; then
    echo "patch-coverage: cargo-llvm-cov is missing — cargo install cargo-llvm-cov" >&2
    exit 2
  fi
  LCOV_PATH="target/patch-coverage-lcov.info"
  echo "Running cargo llvm-cov (instrumented rebuild, this takes a while)..."
  cargo llvm-cov --workspace --lcov --output-path "$LCOV_PATH"
fi

git diff --unified=0 "$BASE...HEAD" -- '*.rs' \
  | python3 scripts/lib/patch_coverage.py "$LCOV_PATH"
