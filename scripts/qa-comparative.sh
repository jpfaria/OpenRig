#!/usr/bin/env bash
# qa-comparative.sh — gate 100% comparativo PR vs base (issue #404)
#
# Filosofia: o gate falha SE E SOMENTE SE o PR PIORA alguma métrica em
# relação a `develop`. Dívida preexistente NUNCA bloqueia. PR que reduz
# dívida sempre passa.
#
# Métricas comparadas (PR vs baseline):
#   1. fmt        — arquivos não-formatados
#   2. clippy     — total de errors do clippy `-D warnings`
#   3. build      — total de errors de cargo build --workspace --all-targets
#   4. test       — testes que falharam (passou na base, quebra no PR)
#   5. complexity — violações dos 4 lints de complexidade
#   6. coverage   — % de linhas cobertas (cargo-llvm-cov)
#
# Falha se qualquer métrica regrediu. Senão passa.
#
# Uso (rodado pelo pr.yml):
#   scripts/qa-comparative.sh <baseline-checkout-path>
#
# Pré-requisitos:
#   - cargo, cargo-llvm-cov, jq instalados
#   - <baseline-checkout-path> contém checkout do merge-base / target ref
#
# Env vars:
#   QA_COV_MARGIN  — pontos percentuais de tolerância em coverage (default 1.0)

set -uo pipefail

BASELINE_DIR="${1:-baseline}"
QA_COV_MARGIN="${QA_COV_MARGIN:-1.0}"

if [ ! -d "$BASELINE_DIR" ]; then
  echo "::error::baseline directory not found: $BASELINE_DIR"
  exit 2
fi

# Use absolute path for baseline so cd-into-it works regardless of cwd.
BASELINE_DIR=$(cd "$BASELINE_DIR" && pwd)
LOG_DIR="${QA_LOG_DIR:-target/qa-logs}"
mkdir -p "$LOG_DIR"

# ─── Helpers ────────────────────────────────────────────────────────────────
# Cada função SEMPRE retorna um único inteiro >= 0 em stdout. Se algo dá
# errado (subshell, cd, cargo morre cedo), o fallback printf garante "0".

# Robusto count via grep; sempre retorna inteiro >= 0.
# grep -c imprime contagem em stdout E pode sair 1 quando 0 matches; nunca
# sobrepor stdout com `|| echo 0` — usar `|| n=0` no caller.
_grep_count() {
  local pattern="$1" file="$2"
  local n
  if n=$(grep -cE "$pattern" "$file" 2>/dev/null); then
    : # n already set
  else
    n=0
  fi
  printf '%d\n' "${n:-0}"
}

# Count fmt-violating files.
count_fmt_errors() {
  local dir="$1" log="$2"
  : > "$log"
  ( cd "$dir" && cargo fmt --all -- --check ) > "$log" 2>&1 || true
  _grep_count '^Diff in ' "$log"
}

# Count clippy errors with -D warnings (without complexity flags).
count_clippy_errors() {
  local dir="$1" log="$2"
  : > "$log"
  (
    cd "$dir" && cargo clippy --workspace --all-targets -- \
      -D warnings \
      -A clippy::cognitive_complexity \
      -A clippy::too_many_lines \
      -A clippy::too_many_arguments \
      -A clippy::type_complexity
  ) > "$log" 2>&1 || true
  _grep_count '^error(\[|:)' "$log"
}

# Count build errors.
count_build_errors() {
  local dir="$1" log="$2"
  : > "$log"
  ( cd "$dir" && cargo build --workspace --all-targets ) > "$log" 2>&1 || true
  _grep_count '^error(\[|:)' "$log"
}

# Count failing tests.
count_test_failures() {
  local dir="$1" log="$2"
  : > "$log"
  ( cd "$dir" && cargo test --workspace --all-targets --no-fail-fast ) \
    > "$log" 2>&1 || true
  local n
  n=$(grep -E '^test result:' "$log" 2>/dev/null \
      | sed -E 's/.*([0-9]+) failed.*/\1/' \
      | awk '{ s += $1 } END { print s+0 }')
  printf '%d\n' "${n:-0}"
}

# Count complexity violations.
count_complexity() {
  local dir="$1" log="$2"
  : > "$log"
  (
    cd "$dir" && cargo clippy --workspace --all-targets -- \
      -A clippy::all \
      -W clippy::cognitive_complexity \
      -W clippy::too_many_lines \
      -W clippy::too_many_arguments \
      -W clippy::type_complexity
  ) > "$log" 2>&1 || true
  _grep_count 'cognitive complexity|too many lines|too many arguments|type complexity' "$log"
}

# Measure coverage % (lines). Always returns a number (0 if anything fails).
measure_coverage() {
  local dir="$1" out="$2"
  ( cd "$dir" && cargo llvm-cov --workspace --json --output-path "$out" ) \
    > /dev/null 2>&1 || true
  local pct=0
  if [ -s "$out" ]; then
    pct=$(jq -r '.data[0].totals.lines.percent // 0' "$out" 2>/dev/null || echo 0)
  fi
  printf '%s\n' "${pct:-0}"
}

# ─── Run all measurements ───────────────────────────────────────────────────

echo "── measuring base ──"
base_fmt=$(count_fmt_errors "$BASELINE_DIR" "$LOG_DIR/base-fmt.log")
base_clippy=$(count_clippy_errors "$BASELINE_DIR" "$LOG_DIR/base-clippy.log")
base_build=$(count_build_errors "$BASELINE_DIR" "$LOG_DIR/base-build.log")
base_tests=$(count_test_failures "$BASELINE_DIR" "$LOG_DIR/base-test.log")
base_complex=$(count_complexity "$BASELINE_DIR" "$LOG_DIR/base-complex.log")
base_cov=$(measure_coverage "$BASELINE_DIR" "$LOG_DIR/base-cov.json")

echo "── measuring PR ──"
pr_fmt=$(count_fmt_errors "." "$LOG_DIR/pr-fmt.log")
pr_clippy=$(count_clippy_errors "." "$LOG_DIR/pr-clippy.log")
pr_build=$(count_build_errors "." "$LOG_DIR/pr-build.log")
pr_tests=$(count_test_failures "." "$LOG_DIR/pr-test.log")
pr_complex=$(count_complexity "." "$LOG_DIR/pr-complex.log")
pr_cov=$(measure_coverage "." "$LOG_DIR/pr-cov.json")

# ─── Compare ────────────────────────────────────────────────────────────────

printf '\n'
printf 'metric        base       pr     verdict\n'
printf '─────────────────────────────────────────\n'

regressed=0
verdict() {
  local name="$1" base="$2" pr="$3"
  if [ "${pr:-0}" -gt "${base:-0}" ]; then
    printf '%-12s %5s   %5s    ❌ regressed\n' "$name" "$base" "$pr"
    regressed=1
  elif [ "${pr:-0}" -lt "${base:-0}" ]; then
    printf '%-12s %5s   %5s    ✅ improved\n' "$name" "$base" "$pr"
  else
    printf '%-12s %5s   %5s    ✅ same\n' "$name" "$base" "$pr"
  fi
}

verdict "fmt"        "$base_fmt"     "$pr_fmt"
verdict "clippy"     "$base_clippy"  "$pr_clippy"
verdict "build"      "$base_build"   "$pr_build"
verdict "test fails" "$base_tests"   "$pr_tests"
verdict "complexity" "$base_complex" "$pr_complex"

# Coverage uses float math + margin.
if awk "BEGIN { exit !($pr_cov < $base_cov - $QA_COV_MARGIN) }"; then
  printf 'coverage     %5.2f%%  %5.2f%%   ❌ regressed (margin: %spp)\n' \
    "$base_cov" "$pr_cov" "$QA_COV_MARGIN"
  regressed=1
elif awk "BEGIN { exit !($pr_cov > $base_cov) }"; then
  printf 'coverage     %5.2f%%  %5.2f%%   ✅ improved\n' "$base_cov" "$pr_cov"
else
  printf 'coverage     %5.2f%%  %5.2f%%   ✅ within margin\n' "$base_cov" "$pr_cov"
fi

printf '\n'
if [ "$regressed" -ne 0 ]; then
  echo "::error::PR regressed at least one metric — see above."
  exit 1
fi
echo "::notice::PR did not regress any metric."
exit 0
