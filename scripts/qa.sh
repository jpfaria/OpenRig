#!/usr/bin/env bash
# qa.sh — OpenRig quality gate (issue #404)
#
# Filosofia: o gate falha quando o PR PIORA métricas, não pelo simples fato
# do código preexistente já estar ruim. Métricas comparativas (complexidade,
# cobertura) ficam no workflow `pr.yml` que compara PR vs `develop`.
#
# Aqui rodam apenas os checks ABSOLUTOS (não regressíveis):
#   1. cargo fmt --all --check          (formatação — fix trivial)
#   2. cargo clippy -D warnings         (warnings — não pode regredir)
#   3. cargo build --workspace          (zero warnings)
#   4. cargo test --workspace           (business validation)
#   5. cargo llvm-cov                   (gera lcov.info; threshold opcional)
#
# Usage:
#   ./scripts/qa.sh             # rodar gate absoluto
#   QA_MIN_COVERAGE=70 ./scripts/qa.sh   # exigir cobertura mínima
#   QA_SKIP_COVERAGE=1 ./scripts/qa.sh   # rodada rápida sem cobertura
#
# Exit: 0 = absolute gate green, 1 = red. Comparativo é em CI.

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

# ─── 2. Clippy (strict warnings; complexity é comparativo no pr.yml) ────────
# Lints de complexidade (cognitive_complexity, too_many_lines,
# too_many_arguments, type_complexity) NÃO entram aqui pra não bloquear por
# dívida preexistente. Regressão de complexidade é comparativa em CI.
run_step "2. cargo clippy" \
  cargo clippy --workspace --all-targets -- -D warnings

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
elif [ "$QA_MIN_COVERAGE" = "0" ]; then
  # Sem piso absoluto: gera lcov.info pra `pr.yml` comparar vs develop.
  run_step "5. cargo llvm-cov (no floor — comparative gate in CI)" \
    cargo llvm-cov --workspace --lcov --output-path lcov.info
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
