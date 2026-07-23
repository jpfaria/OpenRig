#!/bin/bash
# OpenRig macOS local dev installer — build from source and install into /Applications.
#
# Companion to package-macos.sh (which only emits a .dmg) and install-macos.sh
# (which downloads a *released* .dmg via curl). This one is for the developer on
# this machine: it builds a fresh universal bundle from the current checkout and
# drops it straight into /Applications, replacing whatever is there.
#
# Usage: [OPENRIG_PLUGINS_DIR=/path/to/plugins/source] ./scripts/install-macos-local.sh [version]
#   version defaults to "dev". OPENRIG_PLUGINS_DIR is forwarded to package-macos.sh
#   (without it the installed app ships without plugins — a NOTE, not an error).
#
# Issue: #774
set -euo pipefail

VERSION="${1:-dev}"
REPO_ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
cd "$REPO_ROOT"

APP_SRC="dist/OpenRig.app"
APP_DEST="/Applications/OpenRig.app"

if [ "$(uname -s)" != "Darwin" ]; then
    echo "install-macos-local.sh is for macOS only (got $(uname -s))." >&2
    exit 1
fi

# ── 1. Build the universal, ad-hoc-signed bundle (single source of truth) ─────
# package-macos.sh produces dist/OpenRig.app (and a .dmg as a byproduct). We
# reuse it wholesale so the installed app is byte-for-byte what a release ships,
# rather than duplicating the bundle/sign logic here.
echo "==> Building bundle via package-macos.sh (${VERSION})..."
./scripts/package-macos.sh "$VERSION"

if [ ! -d "$APP_SRC" ]; then
    echo "FATAL: expected $APP_SRC after packaging, but it is missing." >&2
    exit 1
fi

# ── 2. Quit any running instance so the replace doesn't hit a busy bundle ─────
echo "==> Quitting any running OpenRig..."
osascript -e 'tell application "OpenRig" to quit' 2>/dev/null || true
# Give a graceful quit a moment; then force any stragglers (e.g. a hung GUI).
for _ in 1 2 3 4 5; do
    pgrep -x openrig >/dev/null 2>&1 || break
    sleep 0.3
done
pkill -x openrig 2>/dev/null || true

# ── 3. Replace the installed app ──────────────────────────────────────────────
echo "==> Installing to ${APP_DEST}..."
if [ -e "$APP_DEST" ] && ! rm -rf "$APP_DEST" 2>/dev/null; then
    echo "FATAL: cannot remove $APP_DEST (permission?). Run as an admin user." >&2
    exit 1
fi
if ! cp -R "$APP_SRC" "$APP_DEST" 2>/dev/null; then
    echo "FATAL: cannot copy into /Applications (permission?). Run as an admin user." >&2
    exit 1
fi

# A locally built bundle has no com.apple.quarantine, but strip it defensively
# in case the tree was ever synced/downloaded — keeps Gatekeeper quiet.
xattr -dr com.apple.quarantine "$APP_DEST" 2>/dev/null || true

# ── 4. Launch it ──────────────────────────────────────────────────────────────
echo "==> Launching OpenRig..."
open -a OpenRig

echo ""
echo "==> Done: installed ${VERSION} to ${APP_DEST} and launched it."
