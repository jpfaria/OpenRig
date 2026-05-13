#!/usr/bin/env bash
# qa.sh — OpenRig quality gate (issue #410, sucessor de #404)
#
# Gate ÚNICO comparativo. Roda local e em CI com mesmo comportamento:
# falha SE E SOMENTE SE o PR piora alguma das 6 métricas em relação ao
# baseline (default: origin/develop). Dívida preexistente NUNCA bloqueia.
#
# Métricas (PR vs baseline):
#   1. fmt        — arquivos não-formatados
#   2. clippy     — total de errors do clippy `-D warnings` (sem complexity)
#   3. build      — total de errors de cargo build --workspace --all-targets
#   4. test       — testes que falharam (passou na base, quebra no PR)
#   5. complexity — violações dos 4 lints de complexidade
#   6. coverage   — % de linhas cobertas (cargo-llvm-cov)
#
# Local:
#   ./scripts/qa.sh
#     → baseline auto-extraído em /tmp/qa-baseline via `git archive origin/develop`.
#     → roda dois cargos do workspace (PR e base). Demora.
#
# CI (.github/workflows/pr.yml):
#   QA_BASELINE_DIR=baseline ./scripts/qa.sh
#     → reusa checkout que o workflow já fez em baseline/.
#
# Env vars:
#   QA_BASELINE_DIR        path do baseline pronto (pula preparação)
#   QA_BASE_REF            git ref a usar como base (default: origin/develop)
#   QA_REFRESH_BASELINE    1 → re-extrai baseline mesmo que exista
#   QA_COV_MARGIN          tolerância de coverage em pp (default: 1.0)
#   QA_LOG_DIR             onde guardar logs por etapa (default: target/qa-logs)
#   QA_FORCE_FULL          1 → força gate completo mesmo em PR sem .rs/Cargo.* (issue #431)
#
# Pré-requisitos: cargo, cargo-llvm-cov, jq.

set -uo pipefail

QA_BASELINE_DIR="${QA_BASELINE_DIR:-}"
QA_BASE_REF="${QA_BASE_REF:-origin/develop}"
QA_COV_MARGIN="${QA_COV_MARGIN:-1.0}"
QA_LOG_DIR="${QA_LOG_DIR:-target/qa-logs}"
QA_REFRESH_BASELINE="${QA_REFRESH_BASELINE:-0}"
QA_FORCE_FULL="${QA_FORCE_FULL:-0}"

cd "$(git rev-parse --show-toplevel 2>/dev/null || pwd)"
mkdir -p "$QA_LOG_DIR"

# ─── Fast-path: skip cargo gates when no Rust changed ───────────────────────
#
# PR que só toca scripts/docs/YAML/etc não pode regredir clippy/build/test/
# coverage do código Rust — matemática simples (não há .rs mudando). Hoje o
# gate roda ~30min mesmo nesse caso por causa do duplo cargo build + llvm-cov.
#
# Detecta o escopo via `git diff --name-only $QA_BASE_REF...HEAD` e cai num
# fast-path que só roda `cargo fmt --check` (validação simbólica, < 5s) e o
# sintaxe-check dos scripts modificados. Issue #431.
#
# Bypass: QA_FORCE_FULL=1 força o gate completo mesmo em PR não-Rust.
RUST_PATH_RE='\.rs$|^Cargo\.|^crates/.*Cargo\.toml$|build\.rs$|^rust-toolchain'

if [ "$QA_FORCE_FULL" != "1" ]; then
  git fetch origin --quiet 2>/dev/null || true
  # Combine 3 sources of "modified files":
  #   1. commits ahead of base ref (already merged into PR's branch)
  #   2. staged changes (git add'd but not committed)
  #   3. working-tree changes (modified but unstaged)
  # Garantir que pré-commit também pegue o fast-path quando o user roda
  # qa.sh local antes de commitar.
  committed_files=$(git diff --name-only "$QA_BASE_REF...HEAD" 2>/dev/null || true)
  staged_files=$(git diff --cached --name-only 2>/dev/null || true)
  worktree_files=$(git diff --name-only 2>/dev/null || true)
  changed_files=$(printf '%s\n%s\n%s\n' "$committed_files" "$staged_files" "$worktree_files" | sort -u | sed '/^$/d')
  if [ -n "$changed_files" ]; then
    if ! echo "$changed_files" | grep -qE "$RUST_PATH_RE"; then
      echo "═══ OpenRig Quality Gate (fast-path) ═══"
      echo "  branch:    $(git rev-parse --abbrev-ref HEAD 2>/dev/null || echo '<detached>')"
      echo "  base ref:  $QA_BASE_REF"
      echo "  scope:     no Rust files touched → skipping cargo gates"
      echo "  override:  QA_FORCE_FULL=1 to run full gate"
      echo ""
      echo "── changed files ──"
      echo "$changed_files" | sed 's/^/  /'
      echo ""
      # Sem `cargo fmt --check` aqui: PR sem `.rs` no diff não pode
      # introduzir nova violação de fmt (workspace Rust intacto). Se
      # rodar fmt-check captura DÍVIDA da develop e bloqueia PR de
      # script/doc por erro alheio — frustração documentada na primeira
      # tentativa de validar este PR #431.
      #
      # Sintaxe shell para qualquer .sh modificado — pega `bash -n`
      # errors que mataram CI antes (catch barato, segundos).
      shell_files=$(echo "$changed_files" | grep -E '\.sh$' || true)
      if [ -n "$shell_files" ]; then
        echo "── bash syntax check ──"
        while IFS= read -r f; do
          [ -z "$f" ] && continue
          [ ! -f "$f" ] && continue
          if ! bash -n "$f" 2>&1; then
            echo "::error::syntax error in $f"
            exit 1
          fi
          echo "  $f: OK"
        done <<< "$shell_files"
      fi
      echo ""
      echo "✅ fast-path passed (no Rust to measure)"
      exit 0
    fi
  fi
fi
# end fast-path

# ─── Baseline provisioning ──────────────────────────────────────────────────
prepare_baseline() {
  local target="$1"
  echo "── preparing baseline ($QA_BASE_REF → $target) ──"
  rm -rf "$target"
  mkdir -p "$target"
  git fetch origin --quiet 2>/dev/null || true
  if ! git archive "$QA_BASE_REF" 2>/dev/null | tar -xC "$target"; then
    echo "::error::failed to extract '$QA_BASE_REF' via git archive — try \`git fetch origin\` first"
    return 1
  fi
}

if [ -z "$QA_BASELINE_DIR" ]; then
  QA_BASELINE_DIR="/tmp/qa-baseline"
  if [ ! -d "$QA_BASELINE_DIR" ] || [ "$QA_REFRESH_BASELINE" = "1" ]; then
    prepare_baseline "$QA_BASELINE_DIR" || exit 1
  else
    echo "── reusing baseline at $QA_BASELINE_DIR (set QA_REFRESH_BASELINE=1 to refresh) ──"
  fi
elif [ ! -d "$QA_BASELINE_DIR" ]; then
  echo "::error::QA_BASELINE_DIR='$QA_BASELINE_DIR' does not exist"
  exit 2
fi

QA_BASELINE_DIR=$(cd "$QA_BASELINE_DIR" && pwd)

# ─── Helpers ────────────────────────────────────────────────────────────────
# Cada função SEMPRE retorna um único inteiro/numero >= 0 em stdout.

# grep -c imprime '0' em stdout E sai 1 quando há 0 matches — não usar
# `|| echo 0` (duplicaria stdout). Encapsular aqui.
_grep_count() {
  local pattern="$1" file="$2"
  local n
  if n=$(grep -cE "$pattern" "$file" 2>/dev/null); then
    :
  else
    n=0
  fi
  printf '%d\n' "${n:-0}"
}

count_fmt_errors() {
  local dir="$1" log="$2"
  : > "$log"
  ( cd "$dir" && cargo fmt --all -- --check ) > "$log" 2>&1 || true
  _grep_count '^Diff in ' "$log"
}

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

count_build_errors() {
  local dir="$1" log="$2"
  : > "$log"
  ( cd "$dir" && cargo build --workspace --all-targets ) > "$log" 2>&1 || true
  _grep_count '^error(\[|:)' "$log"
}

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

# ─── Header ─────────────────────────────────────────────────────────────────
echo ""
echo "═══ OpenRig Quality Gate (comparative) ═══"
echo "  branch:    $(git rev-parse --abbrev-ref HEAD 2>/dev/null || echo '?')"
echo "  base ref:  $QA_BASE_REF"
echo "  baseline:  $QA_BASELINE_DIR"
echo "  cov margin: ${QA_COV_MARGIN}pp"
echo "  logs:      $QA_LOG_DIR/"

# ─── Run all measurements ───────────────────────────────────────────────────
echo ""
echo "── measuring base ──"
base_fmt=$(count_fmt_errors "$QA_BASELINE_DIR" "$QA_LOG_DIR/base-fmt.log")
base_clippy=$(count_clippy_errors "$QA_BASELINE_DIR" "$QA_LOG_DIR/base-clippy.log")
base_build=$(count_build_errors "$QA_BASELINE_DIR" "$QA_LOG_DIR/base-build.log")
base_tests=$(count_test_failures "$QA_BASELINE_DIR" "$QA_LOG_DIR/base-test.log")
base_complex=$(count_complexity "$QA_BASELINE_DIR" "$QA_LOG_DIR/base-complex.log")
base_cov=$(measure_coverage "$QA_BASELINE_DIR" "$QA_LOG_DIR/base-cov.json")

echo "── measuring PR ──"
pr_fmt=$(count_fmt_errors "." "$QA_LOG_DIR/pr-fmt.log")
pr_clippy=$(count_clippy_errors "." "$QA_LOG_DIR/pr-clippy.log")
pr_build=$(count_build_errors "." "$QA_LOG_DIR/pr-build.log")
pr_tests=$(count_test_failures "." "$QA_LOG_DIR/pr-test.log")
pr_complex=$(count_complexity "." "$QA_LOG_DIR/pr-complex.log")
pr_cov=$(measure_coverage "." "$QA_LOG_DIR/pr-cov.json")

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
