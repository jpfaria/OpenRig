#!/bin/bash
# Tests for scripts/lib/console-binaries.sh (issue #741).
#
# The platform packagers historically shipped ONLY the GUI binary. This
# library is the single source of truth for the extra console-style
# binaries (headless console + offline render) that every packager must
# also build and stage. The test pins:
#   - the SOT list (cargo pkg -> built basename -> installed basename)
#   - the cargo build flags derived from it
#   - stage_console_binaries copying each built binary to its installed name
#   - a missing built binary being FATAL (a packager must not ship a GUI-only
#     bundle silently)
#
# Run: ./scripts/tests/console_bundle_test.sh
set -uo pipefail

SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
LIB="$SCRIPT_DIR/../lib/console-binaries.sh"
TSV="$SCRIPT_DIR/../lib/console-binaries.tsv"

FAILURES=0
fail() { echo "FAIL: $1" >&2; FAILURES=$((FAILURES + 1)); }
pass() { echo "ok:   $1"; }

if [ ! -f "$LIB" ]; then
    echo "FAIL: $LIB does not exist" >&2
    exit 1
fi
if [ ! -f "$TSV" ]; then
    echo "FAIL: $TSV does not exist" >&2
    exit 1
fi
# shellcheck source=../lib/console-binaries.sh
source "$LIB"

TMP="$(mktemp -d)"
trap 'rm -rf "$TMP"' EXIT

# ── console_binaries: SOT lists console + console-rig + render ───────────────
OUT="$(console_binaries)"
for expected in \
    "adapter-console openrig-console" \
    "adapter-console-rig openrig-console-rig" \
    "openrig-render openrig-render"; do
    if printf '%s\n' "$OUT" | grep -qx "$expected"; then
        pass "console_binaries lists '$expected'"
    else
        fail "console_binaries missing '$expected'; got: $OUT"
    fi
done

# ── console_binaries: exactly three entries, no GUI binary leaking in ────────
N="$(console_binaries | grep -c .)"
if [ "$N" = "3" ]; then
    pass "console_binaries lists exactly 3 binaries"
else
    fail "console_binaries should list 3 binaries, got $N"
fi
if console_binaries | awk '{print $2}' | grep -qx openrig; then
    fail "console_binaries must not install over the GUI 'openrig' binary"
else
    pass "console_binaries excludes the GUI binary"
fi

# ── console_build_flags: one -p per cargo package ───────────────────────────
FLAGS="$(console_build_flags)"
for pkg in adapter-console adapter-console-rig adapter-render; do
    case " $FLAGS " in
        *" -p $pkg "*) pass "console_build_flags includes -p $pkg" ;;
        *) fail "console_build_flags missing -p $pkg; got: $FLAGS" ;;
    esac
done

# ── stage_console_binaries: copies built basenames to installed names ───────
REL="$TMP/release"
DEST="$TMP/dest"
mkdir -p "$REL" "$DEST"
for b in adapter-console adapter-console-rig openrig-render; do
    printf '#!/bin/sh\n' > "$REL/$b"
done
if stage_console_binaries "$REL" "$DEST" "" >/dev/null; then
    pass "stage_console_binaries returns 0 when all binaries present"
else
    fail "stage_console_binaries failed despite all binaries present"
fi
for inst in openrig-console openrig-console-rig openrig-render; do
    if [ -f "$DEST/$inst" ]; then
        pass "stage_console_binaries staged $inst"
    else
        fail "stage_console_binaries did not stage $inst"
    fi
done
# the GUI binary's name must NOT appear (we only stage the extras)
if [ -f "$DEST/adapter-console" ]; then
    fail "stage_console_binaries left the built basename instead of the installed name"
else
    pass "stage_console_binaries renames to installed basename"
fi

# ── stage_console_binaries: .exe suffix for Windows-style staging ───────────
RELW="$TMP/release-win"
DESTW="$TMP/dest-win"
mkdir -p "$RELW" "$DESTW"
for b in adapter-console adapter-console-rig openrig-render; do
    printf 'MZ' > "$RELW/$b.exe"
done
if stage_console_binaries "$RELW" "$DESTW" ".exe" >/dev/null \
    && [ -f "$DESTW/openrig-console.exe" ] \
    && [ -f "$DESTW/openrig-render.exe" ]; then
    pass "stage_console_binaries honors the .exe suffix"
else
    fail "stage_console_binaries did not stage .exe binaries"
fi

# ── stage_console_binaries: a missing built binary is FATAL ─────────────────
RELMISS="$TMP/release-missing"
DESTMISS="$TMP/dest-missing"
mkdir -p "$RELMISS" "$DESTMISS"
printf '#!/bin/sh\n' > "$RELMISS/adapter-console"   # only one of three present
if stage_console_binaries "$RELMISS" "$DESTMISS" "" >/dev/null 2>&1; then
    fail "stage_console_binaries must fail when a built binary is missing"
else
    pass "stage_console_binaries fails loud on a missing built binary"
fi

echo ""
if [ "$FAILURES" -gt 0 ]; then
    echo "$FAILURES failure(s)" >&2
    exit 1
fi
echo "all tests passed"
