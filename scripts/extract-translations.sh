#!/usr/bin/env bash
# extract-translations.sh — refresh translations/adapter-gui.pot from the
# current @tr(...) usage in every .slint file under crates/adapter-gui/ui/,
# then merge the new template into each per-locale .po so existing
# translations are preserved.
#
# Requires:
#   - slint-tr-extractor (from `cargo install slint-tr-extractor` or via
#     the Slint CLI tools — bundled with `slint-build` since 1.x)
#   - gettext tools: msgmerge, msgcat (preinstalled on macOS via Xcode CLT,
#     `apt install gettext` on Linux, chocolatey/scoop on Windows)
#
# Usage:
#   scripts/extract-translations.sh
#
# Run this whenever you add or modify @tr() strings. The CI can also run
# it with --check to detect drift.
set -euo pipefail

REPO_ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
TRANSLATIONS="$REPO_ROOT/crates/adapter-gui/translations"
DOMAIN="adapter-gui"
POT="$TRANSLATIONS/$DOMAIN.pot"

cd "$REPO_ROOT"

# --check: regenerate, then fail if catalogs drift from what's committed.
# This is the durable guard against the root cause of issue #446 — the
# script having silently never run while .slint keys were renamed.
CHECK_MODE=0
[ "${1:-}" = "--check" ] && CHECK_MODE=1

# Make sure slint-tr-extractor is on PATH; install if missing.
if ! command -v slint-tr-extractor >/dev/null 2>&1; then
  echo "slint-tr-extractor not found — installing via cargo..."
  cargo install --locked slint-tr-extractor
fi

# Make sure msgmerge is on PATH.
if ! command -v msgmerge >/dev/null 2>&1; then
  echo "ERROR: msgmerge not found. Install gettext tools:" >&2
  echo "  macOS:   brew install gettext (and link if needed)" >&2
  echo "  Linux:   apt install gettext" >&2
  echo "  Windows: choco install gettext  (or scoop install gettext)" >&2
  exit 1
fi

echo "→ extracting strings from .slint files…"
SLINT_FILES=$(find crates/adapter-gui/ui -name '*.slint' -not -path '*/modules/*' | sort)
# slint-tr-extractor accepts multiple files and writes a combined .pot.
# shellcheck disable=SC2086  # we want word-splitting
slint-tr-extractor \
  --default-domain "$DOMAIN" \
  --package-name "OpenRig" \
  -o "$POT" \
  $SLINT_FILES

# slint-tr-extractor stamps a fresh POT-Creation-Date every run, which would
# make --check perpetually fail in CI on pure timestamp churn. Pin it so the
# .pot is reproducible — content changes still surface, noise doesn't.
if [ -f "$POT" ]; then
  sed -i.bak 's/^"POT-Creation-Date:.*/"POT-Creation-Date: 2026-01-01 00:00+0000\\n"/' "$POT"
  rm -f "$POT.bak"
  # Keep the catalog FLAT (no per-component msgctxt). The GUI compiles with
  # slint's DefaultTranslationContext::None, so @tr(...) looks up by BARE msgid
  # at runtime. Newer slint-tr-extractor emits `msgctxt "<Component>"` per
  # string, which would never match the None-context lookup → the UI shows the
  # raw key (e.g. "TONE-DOCTOR-TITLE"). Strip it so every entry stays keyed by
  # msgid alone. A label reused in two components then collapses to duplicate
  # msgids (UI labels are context-independent here), so msguniq folds them —
  # otherwise msgmerge below aborts with "duplicate message definition".
  sed -i.bak '/^msgctxt /d' "$POT"
  rm -f "$POT.bak"
  msguniq --use-first -o "$POT.uniq" "$POT" && mv "$POT.uniq" "$POT"
fi

echo "→ updated $POT"

# Sync each per-locale .po against the new template.
for lang_dir in "$TRANSLATIONS"/*/; do
  lang="$(basename "$lang_dir")"
  # Slint's with_bundled_translations expects .po under <lang>/LC_MESSAGES/.
  po="${lang_dir}LC_MESSAGES/${DOMAIN}.po"
  if [ -f "$po" ]; then
    echo "→ merging into $lang/LC_MESSAGES/$DOMAIN.po…"
    msgmerge --update --backup=none --no-fuzzy-matching "$po" "$POT"
    # A pure key/context rename orphans the old entry as obsolete (#~).
    # po_reconcile recovers its human translation onto the new active
    # entry sharing the same msgid (UI labels are context-independent:
    # "Inputs" is "Inputs" in every window), then drops obsolete so the
    # catalog can't rot the way it did before issue #446 (178 dead
    # entries had accumulated). Done in-script (not via `msgattrib
    # --no-obsolete`) because some non-GNU msgattrib builds wipe active
    # msgstr — po_reconcile is deterministic and dependency-free.
    python3 "$REPO_ROOT/scripts/po_reconcile.py" "$po"
  fi
done

if [ "$CHECK_MODE" = "1" ]; then
  if ! git diff --quiet -- "$TRANSLATIONS"; then
    echo "✗ translation catalogs are stale — run scripts/extract-translations.sh and commit" >&2
    git --no-pager diff --stat -- "$TRANSLATIONS" >&2
    exit 1
  fi
  echo "✓ translation catalogs in sync"
else
  echo "✓ translations refreshed"
fi
