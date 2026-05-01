#!/usr/bin/env bash
# validate.sh — static quality gate for OpenRig
#
# Usage:
#   ./scripts/validate.sh file1 file2 ...   check specific files
#   ./scripts/validate.sh                   check git-diff files (staged + unstaged)
#
# Checks:
#   1. File size (lines)
#   2. Rust formatting (cargo fmt --check)
#   3. Rust linting   (cargo clippy -D warnings)
#   4. Slint compilation (cargo check -p adapter-gui)
#
# Exit: 0 = pass, 1 = violations found

set -uo pipefail

RUST_MAX_LINES=600
SLINT_MAX_LINES=500

RED='\033[0;31m'
YELLOW='\033[1;33m'
GREEN='\033[0;32m'
BOLD='\033[1m'
NC='\033[0m'

ERRORS=0
WARNINGS=0
SEEN_CRATES=""   # space-separated list of unique package names
SLINT_CHANGED=false

# ─── Known debt files — warn instead of fail ────────────────────────────────
# These files already exceed the limit. Do NOT add new ones here.
# Instead: refactor the file before extending it.
DEBT_FILES="
crates/adapter-gui/src/lib.rs
crates/engine/src/runtime.rs
crates/infra-cpal/src/lib.rs
crates/infra-yaml/src/lib.rs
crates/project/src/block.rs
crates/block-core/src/param.rs
crates/application/src/validate.rs
crates/block-core/src/lib.rs
crates/adapter-gui/src/block_editor.rs
crates/ir/src/lib.rs
crates/infra-filesystem/src/lib.rs
crates/block-gain/src/native_ibanez_ts9.rs
crates/vst3/src/host.rs
crates/project/src/chain.rs
crates/project/src/catalog.rs
crates/adapter-gui/src/visual_config/mod.rs
crates/block-mod/src/lib.rs
crates/block-dyn/src/lib.rs
crates/block-delay/src/lib.rs
crates/adapter-gui/ui/pages/project_chains.slint
crates/adapter-gui/ui/app-window.slint
crates/adapter-gui/ui/pages/block_panel_editor.slint
crates/adapter-gui/ui/pages/compact_chain_view.slint
crates/adapter-gui/ui/touch_main.slint
crates/adapter-gui/ui/pages/chain_row.slint
"

is_debt() {
  local norm
  norm=$(echo "$1" | sed "s|$(pwd)/||")
  echo "$DEBT_FILES" | grep -qF "$norm"
}

add_crate() {
  local pkg="$1"
  [ -z "$pkg" ] && return
  echo "$SEEN_CRATES" | grep -qw "$pkg" || SEEN_CRATES="$SEEN_CRATES $pkg"
}

fail() { echo -e "  ${RED}✗ FAIL${NC} $1"; ERRORS=$((ERRORS + 1)); }
warn() { echo -e "  ${YELLOW}⚠ WARN${NC} $1"; WARNINGS=$((WARNINGS + 1)); }
ok()   { echo -e "  ${GREEN}✓ OK${NC}   $1"; }

find_package_name() {
  local file="$1"
  local dir
  dir=$(dirname "$file")
  while [ "$dir" != "." ] && [ "$dir" != "/" ] && [ -n "$dir" ]; do
    if [ -f "$dir/Cargo.toml" ] && grep -q '^\[package\]' "$dir/Cargo.toml" 2>/dev/null; then
      grep '^name' "$dir/Cargo.toml" | head -1 | sed 's/name *= *"\(.*\)"/\1/' | tr -d ' '
      return
    fi
    dir=$(dirname "$dir")
  done
}

# ─── Resolve files to check ─────────────────────────────────────────────────
cd "$(git rev-parse --show-toplevel 2>/dev/null || pwd)"

if [ $# -gt 0 ]; then
  # Expand directories recursively; keep plain files as-is
  FILES=""
  for arg in "$@"; do
    if [ -d "$arg" ]; then
      found=$(find "$arg" \( -name "*.rs" -o -name "*.slint" \) | sort)
      FILES="$FILES $found"
    elif [ -f "$arg" ]; then
      FILES="$FILES $arg"
    else
      echo "validate.sh: '$arg' not found, skipping." >&2
    fi
  done
  FILES=$(echo "$FILES" | tr ' ' '\n' | sort -u | grep -v '^$')
else
  FILES=$(
    { git diff --cached --name-only 2>/dev/null; git diff --name-only 2>/dev/null; } \
    | sort -u | grep -v '^$' || true
  )
fi

if [ -z "$FILES" ]; then
  echo "validate.sh: no files to check (pass files as args or stage/modify files)."
  exit 0
fi

FILE_COUNT=$(echo "$FILES" | wc -w | tr -d ' ')
echo ""
echo -e "${BOLD}═══ OpenRig Validate ═══${NC}  ($FILE_COUNT file(s))"
echo ""

# ─── 1. FILE SIZE ───────────────────────────────────────────────────────────
echo -e "${BOLD}── 1. File Size ──${NC}"
for file in $FILES; do
  [ -f "$file" ] || continue

  ext="${file##*.}"
  [[ "$ext" == "rs" || "$ext" == "slint" ]] || continue

  lines=$(wc -l < "$file" | tr -d ' ')
  name=$(basename "$file")

  case "$ext" in
    rs)
      limit=$RUST_MAX_LINES
      label="Rust"
      pkg=$(find_package_name "$file")
      add_crate "$pkg"
      ;;
    slint)
      limit=$SLINT_MAX_LINES
      label="Slint"
      SLINT_CHANGED=true
      ;;
    *) continue ;;
  esac

  if [ "$lines" -gt "$limit" ]; then
    if is_debt "$file"; then
      warn "$name ($lines lines > $limit $label limit) — known debt, do not grow"
    else
      fail "$name ($lines lines > $limit $label limit) — split into smaller modules"
    fi
  else
    ok "$name ($lines lines)"
  fi
done

# ─── 2. RUST FORMATTING (per-file, not per-crate) ───────────────────────────
RS_FILES=$(echo "$FILES" | tr ' ' '\n' | grep '\.rs$' | grep -v '^$' || true)
if [ -n "$RS_FILES" ]; then
  echo ""
  echo -e "${BOLD}── 2. Rust Formatting ──${NC}"
  for file in $RS_FILES; do
    [ -f "$file" ] || continue
    if rustfmt --check "$file" > /dev/null 2>&1; then
      ok "$(basename "$file"): formatting OK"
    else
      fail "$(basename "$file"): formatting violations — run: rustfmt $file"
    fi
  done
fi

# ─── 3. RUST CLIPPY ─────────────────────────────────────────────────────────
if [ -n "$(echo "$SEEN_CRATES" | tr -d ' ')" ]; then
  echo ""
  echo -e "${BOLD}── 3. Rust Clippy ──${NC}"
  for pkg in $SEEN_CRATES; do
    if cargo clippy -p "$pkg" -- -D warnings 2>/dev/null; then
      ok "$pkg: clippy OK"
    else
      fail "$pkg: clippy violations — fix warnings before committing"
    fi
  done
fi

# ─── 4. SLINT COMPILATION ───────────────────────────────────────────────────
if $SLINT_CHANGED; then
  echo ""
  echo -e "${BOLD}── 4. Slint Compilation ──${NC}"
  if cargo check -p adapter-gui 2>/dev/null; then
    ok "adapter-gui: Slint OK"
  else
    fail "adapter-gui: Slint compilation errors"
  fi
fi

# ─── RESULT ─────────────────────────────────────────────────────────────────
echo ""
if [ "$ERRORS" -gt 0 ]; then
  echo -e "${RED}${BOLD}═══ VALIDATE FAILED: $ERRORS error(s), $WARNINGS warning(s) ═══${NC}"
  echo "  Fix all errors before committing. Warnings = known debt, do not grow."
  exit 1
else
  if [ "$WARNINGS" -gt 0 ]; then
    echo -e "${YELLOW}${BOLD}═══ VALIDATE PASSED with $WARNINGS warning(s) ═══${NC}"
    echo "  Warnings = known debt files. Do not add more lines to them."
  else
    echo -e "${GREEN}${BOLD}═══ VALIDATE PASSED ═══${NC}"
  fi
  exit 0
fi
