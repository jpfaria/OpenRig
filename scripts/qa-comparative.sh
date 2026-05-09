#!/usr/bin/env bash
# qa-comparative.sh — gate comparativo PR vs base (issue #404)
#
# Filosofia: gate falha quando o PR PIORA métricas vs `develop`. Se a métrica
# já era ruim e o PR não toca, passa. Se o PR melhora, passa. Se piora, falha.
#
# Métricas comparadas:
#   - Complexidade: contagem de violações de clippy::cognitive_complexity,
#     too_many_lines, too_many_arguments, type_complexity. PR > base → falha.
#   - Cobertura: % de linhas (cargo-llvm-cov). PR < base - margem → falha.
#
# Uso (rodado pelo pr.yml):
#   scripts/qa-comparative.sh <baseline-checkout-path>
#
# Pré-requisitos:
#   - cargo, cargo-llvm-cov, jq instalados
#   - <baseline-checkout-path> contém um checkout do merge-base / target ref
#
# Env vars:
#   QA_COV_MARGIN  — pontos percentuais de tolerância (default 1.0)

set -uo pipefail

BASELINE_DIR="${1:-baseline}"
QA_COV_MARGIN="${QA_COV_MARGIN:-1.0}"

if [ ! -d "$BASELINE_DIR" ]; then
  echo "::error::baseline directory not found: $BASELINE_DIR"
  exit 2
fi

count_complexity() {
  local dir="$1"
  (
    cd "$dir"
    cargo clippy --workspace --all-targets --quiet -- \
      -A clippy::all \
      -W clippy::cognitive_complexity \
      -W clippy::too_many_lines \
      -W clippy::too_many_arguments \
      -W clippy::type_complexity 2>&1 \
      | grep -cE 'cognitive complexity|too many lines|too many arguments|type complexity' \
      || true
  )
}

measure_coverage() {
  local dir="$1"
  local out
  out=$(mktemp)
  (
    cd "$dir"
    cargo llvm-cov --workspace --json --output-path "$out" >/dev/null 2>&1 || true
  )
  if [ -s "$out" ]; then
    jq -r '.data[0].totals.lines.percent // 0' "$out"
  else
    echo 0
  fi
  rm -f "$out"
}

echo "── measuring base ($BASELINE_DIR) ──"
base_complex=$(count_complexity "$BASELINE_DIR")
base_cov=$(measure_coverage "$BASELINE_DIR")

echo "── measuring PR (.) ──"
pr_complex=$(count_complexity ".")
pr_cov=$(measure_coverage ".")

printf '\n'
printf 'metric        base       pr\n'
printf 'complexity   %4s     %4s\n' "$base_complex" "$pr_complex"
printf 'coverage    %5.2f%%  %5.2f%%\n' "$base_cov" "$pr_cov"
printf '\n'

regress=0

if [ "${pr_complex:-0}" -gt "${base_complex:-0}" ]; then
  echo "::error::complexity regressed: $base_complex → $pr_complex"
  regress=1
else
  echo "complexity OK ($base_complex → $pr_complex)"
fi

# coverage compared with awk for float math
if awk "BEGIN { exit !($pr_cov < $base_cov - $QA_COV_MARGIN) }"; then
  printf '::error::coverage regressed: %.2f%% → %.2f%% (margin: %spp)\n' \
    "$base_cov" "$pr_cov" "$QA_COV_MARGIN"
  regress=1
else
  printf 'coverage OK (%.2f%% → %.2f%%, margin: %spp)\n' \
    "$base_cov" "$pr_cov" "$QA_COV_MARGIN"
fi

exit $regress
