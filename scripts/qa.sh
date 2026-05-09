#!/usr/bin/env bash
# qa.sh — OpenRig quality gate (issue #404)
#
# Single source of truth for the quality gate. Same script runs locally
# (before push) and in CI (.github/workflows/pr.yml). If any step fails,
# nothing goes through.
#
# Usage:
#   ./scripts/qa.sh             # run full gate
#   QA_MIN_COVERAGE=70 ./scripts/qa.sh
#   QA_SKIP_COVERAGE=1 ./scripts/qa.sh   # quick run (no coverage)
#
# Steps:
#   1. cargo fmt --all --check               (formatting)
#   2. cargo clippy --workspace -D warnings  (lints + complexity)
#   3. cargo build --workspace               (zero warnings)
#   4. cargo test --workspace                (business validation)
#   5. cargo llvm-cov --fail-under-lines N   (coverage floor)
#
# Exit: 0 = gate green, 1 = gate red.

set -uo pipefail

# ─── Config ─────────────────────────────────────────────────────────────────
QA_MIN_COVERAGE="${QA_MIN_COVERAGE:-0}"
QA_SKIP_COVERAGE="${QA_SKIP_COVERAGE:-0}"
QA_LOG_DIR="${QA_LOG_DIR:-target/qa-logs}"

RED='\033[0;31m'
GREEN='\033[0;32m'
YELLOW='\033[1;33m'
BOLD='\033[1m'
NC='\033[0m'

cd "$(git rev-parse --show-toplevel 2>/dev/null || pwd)"
mkdir -p "$QA_LOG_DIR"

FAILED_STEPS=""

run_step() {
  local name="$1"; shift
  local log="$QA_LOG_DIR/${name// /_}.log"

  echo ""
  echo -e "${BOLD}── ${name} ──${NC}"
  if "$@" > "$log" 2>&1; then
    echo -e "  ${GREEN}✓ OK${NC}   $name"
  else
    echo -e "  ${RED}✗ FAIL${NC} $name"
    echo -e "  ${YELLOW}log:${NC} $log"
    tail -n 30 "$log" | sed 's/^/    /'
    FAILED_STEPS="$FAILED_STEPS\n  - $name (log: $log)"
  fi
}

# ─── Header ─────────────────────────────────────────────────────────────────
echo ""
echo -e "${BOLD}═══ OpenRig Quality Gate ═══${NC}"
echo "  branch:    $(git rev-parse --abbrev-ref HEAD 2>/dev/null || echo '?')"
echo "  coverage:  >= ${QA_MIN_COVERAGE}% (skip=${QA_SKIP_COVERAGE})"
echo "  logs:      $QA_LOG_DIR/"

# ─── 1. Formatting ──────────────────────────────────────────────────────────
run_step "1. cargo fmt --check" \
  cargo fmt --all -- --check

# ─── 2. Clippy + complexity lints ───────────────────────────────────────────
run_step "2. cargo clippy" \
  cargo clippy --workspace --all-targets -- \
    -D warnings \
    -W clippy::cognitive_complexity \
    -W clippy::too_many_lines \
    -W clippy::too_many_arguments \
    -W clippy::type_complexity \
    -W clippy::module_inception \
    -W clippy::wildcard_imports

# ─── 3. Build (zero warnings invariant) ─────────────────────────────────────
run_step "3. cargo build --workspace" \
  cargo build --workspace --all-targets

# ─── 4. Tests (business validation) ─────────────────────────────────────────
run_step "4. cargo test --workspace" \
  cargo test --workspace --all-targets

# ─── 5. Coverage (line floor) ───────────────────────────────────────────────
if [ "$QA_SKIP_COVERAGE" = "1" ]; then
  echo ""
  echo -e "${BOLD}── 5. cargo llvm-cov ──${NC}"
  echo -e "  ${YELLOW}skipped${NC} (QA_SKIP_COVERAGE=1)"
elif ! command -v cargo-llvm-cov >/dev/null 2>&1; then
  echo ""
  echo -e "${BOLD}── 5. cargo llvm-cov ──${NC}"
  echo -e "  ${YELLOW}skipped${NC} (cargo-llvm-cov not installed — \`cargo install cargo-llvm-cov\`)"
else
  run_step "5. cargo llvm-cov" \
    cargo llvm-cov --workspace \
      --fail-under-lines "$QA_MIN_COVERAGE" \
      --lcov --output-path lcov.info
fi

# ─── Result ─────────────────────────────────────────────────────────────────
echo ""
if [ -n "$FAILED_STEPS" ]; then
  echo -e "${RED}${BOLD}═══ QA GATE FAILED ═══${NC}"
  echo -e "  failed steps:$FAILED_STEPS"
  echo ""
  echo "  Fix locally and re-run \`./scripts/qa.sh\` until green before pushing."
  exit 1
fi

echo -e "${GREEN}${BOLD}═══ QA GATE PASSED ═══${NC}"
exit 0
