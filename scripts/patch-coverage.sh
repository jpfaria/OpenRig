#!/usr/bin/env bash
# Patch coverage before the push — the same number `codecov/patch` will report.
#
# CI scores the lines a branch ADDS against the base's coverage. Discovering
# that in CI costs a full run per attempt, so measure it here: instrumented
# run once, then score the diff.
#
#   ./scripts/patch-coverage.sh                 # vs origin/main
#   ./scripts/patch-coverage.sh origin/develop  # vs another base
#   ./scripts/patch-coverage.sh --files         # list the files still missing
#
# Skip it the way the other gates are skipped:
#
#   PATCH_COV_OFF=1 ./scripts/patch-coverage.sh
#
# Reuses an existing report when it is newer than the last commit; pass
# --fresh to force the instrumented run again.
set -euo pipefail

if [ "${PATCH_COV_OFF:-0}" = "1" ]; then
    echo "patch-coverage: skipped (PATCH_COV_OFF=1)"
    exit 0
fi

ROOT="$(git rev-parse --show-toplevel)"
cd "$ROOT"

BASE="origin/main"
FILES=""
FRESH=""
for arg in "$@"; do
    case "$arg" in
        --files) FILES="--files" ;;
        --fresh) FRESH="1" ;;
        -*) echo "unknown flag: $arg" >&2; exit 2 ;;
        *) BASE="$arg" ;;
    esac
done

if ! command -v cargo-llvm-cov >/dev/null 2>&1; then
    echo "patch-coverage: cargo-llvm-cov is missing — cargo install cargo-llvm-cov" >&2
    exit 1
fi

git rev-parse --verify --quiet "$BASE" >/dev/null || {
    echo "patch-coverage: base '$BASE' not found — git fetch it first" >&2
    exit 1
}

LCOV="target/patch-coverage/lcov.info"
mkdir -p "$(dirname "$LCOV")"

# The instrumented run is the slow part; only redo it when the tree moved.
if [ -n "$FRESH" ] || [ ! -f "$LCOV" ] || [ -n "$(git status --porcelain -- '*.rs')" ] \
    || [ "$(git log -1 --format=%ct)" -gt "$(stat -f %m "$LCOV" 2>/dev/null || echo 0)" ]; then
    echo "==> cargo llvm-cov (instrumented run, this is the slow part)…"
    cargo llvm-cov --workspace --lcov --output-path "$LCOV"
else
    echo "==> reusing $LCOV (pass --fresh to rebuild)"
fi

python3 "$ROOT/scripts/lib/patch-coverage.py" "$LCOV" "$BASE" $FILES
