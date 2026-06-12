#!/bin/bash
# Tests for scripts/lib/plugins-bundle.sh (issue #709).
# Run: ./scripts/tests/plugins_bundle_test.sh
set -uo pipefail

SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
LIB="$SCRIPT_DIR/../lib/plugins-bundle.sh"

FAILURES=0
fail() { echo "FAIL: $1" >&2; FAILURES=$((FAILURES + 1)); }
pass() { echo "ok:   $1"; }

if [ ! -f "$LIB" ]; then
    echo "FAIL: $LIB does not exist" >&2
    exit 1
fi
# shellcheck source=../lib/plugins-bundle.sh
source "$LIB"

TMP="$(mktemp -d)"
trap 'rm -rf "$TMP"' EXIT

# ── plugins_src_dir: default is plugins/source ──────────────────────────────
unset OPENRIG_PLUGINS_DIR
if [ "$(plugins_src_dir)" = "plugins/source" ]; then
    pass "plugins_src_dir defaults to plugins/source"
else
    fail "plugins_src_dir default: got '$(plugins_src_dir)'"
fi

# ── plugins_src_dir: OPENRIG_PLUGINS_DIR overrides the default ──────────────
mkdir -p "$TMP/custom-plugins"
if [ "$(OPENRIG_PLUGINS_DIR="$TMP/custom-plugins" plugins_src_dir)" = "$TMP/custom-plugins" ]; then
    pass "plugins_src_dir honors OPENRIG_PLUGINS_DIR"
else
    fail "plugins_src_dir override: got '$(OPENRIG_PLUGINS_DIR="$TMP/custom-plugins" plugins_src_dir)'"
fi

# ── plugins_src_dir: explicit override pointing nowhere is FATAL ────────────
if OPENRIG_PLUGINS_DIR="$TMP/does-not-exist" plugins_src_dir >/dev/null 2>&1; then
    fail "plugins_src_dir must fail when OPENRIG_PLUGINS_DIR is not a directory"
else
    pass "plugins_src_dir fails loud on missing override dir"
fi

# ── bundle_plugins: stages from a custom src, drops non-target platforms ────
SRC="$TMP/src"
DEST="$TMP/dest/plugins"
mkdir -p "$SRC/lv2/myplugin/platform/macos-universal" \
         "$SRC/lv2/myplugin/platform/linux-x86_64" \
         "$SRC/lv2/myplugin/platform/windows-x86_64"
touch "$SRC/lv2/myplugin/manifest.yaml" \
      "$SRC/lv2/myplugin/platform/macos-universal/p.dylib" \
      "$SRC/lv2/myplugin/platform/linux-x86_64/p.so" \
      "$SRC/lv2/myplugin/platform/windows-x86_64/p.dll"
mkdir -p "$TMP/dest"

bundle_plugins "$SRC" "$DEST" "linux-*" "windows-*" >/dev/null

[ -f "$DEST/lv2/myplugin/manifest.yaml" ] \
    && pass "bundle_plugins copies manifests from custom src" \
    || fail "manifest.yaml missing in dest"
[ -f "$DEST/lv2/myplugin/platform/macos-universal/p.dylib" ] \
    && pass "bundle_plugins keeps target platform dirs" \
    || fail "macos-universal dropped (should be kept)"
[ ! -d "$DEST/lv2/myplugin/platform/linux-x86_64" ] \
    && pass "bundle_plugins drops linux platform dirs" \
    || fail "linux-x86_64 not dropped"
[ ! -d "$DEST/lv2/myplugin/platform/windows-x86_64" ] \
    && pass "bundle_plugins drops windows platform dirs" \
    || fail "windows-x86_64 not dropped"

# ── bundle_plugins: missing src is a NOTE, not an error ──────────────────────
if OUT="$(bundle_plugins "$TMP/nope" "$TMP/dest2" "linux-*")"; then
    case "$OUT" in
        *NOTE*) pass "bundle_plugins notes missing src and returns 0" ;;
        *) fail "bundle_plugins missing-src output lacks NOTE: '$OUT'" ;;
    esac
else
    fail "bundle_plugins must return 0 on missing src"
fi
[ ! -d "$TMP/dest2" ] \
    && pass "bundle_plugins creates nothing on missing src" \
    || fail "dest created despite missing src"

echo ""
if [ "$FAILURES" -gt 0 ]; then
    echo "$FAILURES failure(s)" >&2
    exit 1
fi
echo "all tests passed"
