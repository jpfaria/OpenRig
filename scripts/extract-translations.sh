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

echo "→ updated $POT"

# Sync each per-locale .po against the new template.
for lang_dir in "$TRANSLATIONS"/*/; do
  lang="$(basename "$lang_dir")"
  po="$lang_dir$DOMAIN.po"
  if [ -f "$po" ]; then
    echo "→ merging into $lang/$DOMAIN.po…"
    msgmerge --update --backup=none --no-fuzzy-matching "$po" "$POT"
  fi
done

echo "✓ translations refreshed"
