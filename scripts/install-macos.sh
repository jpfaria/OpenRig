#!/bin/bash
# OpenRig macOS installer — no Apple Developer certificate involved.
#
# Why this exists: the released .app is only ad-hoc signed (no paid Apple
# notarization). A browser/Finder download tags it com.apple.quarantine
# and Gatekeeper refuses to open it. Nothing the app ships can run before
# that block (Gatekeeper evaluates before our code; quarantine is applied
# post-download) — so the only "automatic" path is this script, fetched
# with curl (curl does NOT set quarantine, so the script itself runs
# freely) and run by the user:
#
#   curl -fsSL https://raw.githubusercontent.com/jpfaria/OpenRig/develop/scripts/install-macos.sh | bash
#
# Optional: pass a version to pin it, otherwise the latest release is used:
#   ... | bash -s -- v0.1.0-dev.19
#
# Issue: #459
set -euo pipefail

REPO="jpfaria/OpenRig"
VERSION="${1:-latest}"
APP_DEST="/Applications/OpenRig.app"

if [ "$(uname -s)" != "Darwin" ]; then
    echo "install-macos.sh is for macOS only (got $(uname -s))." >&2
    exit 1
fi

api="https://api.github.com/repos/${REPO}/releases/$(
    [ "$VERSION" = "latest" ] && echo "latest" || echo "tags/${VERSION}"
)"

echo "==> Resolving macOS .dmg from ${REPO} (${VERSION})..."
dmg_url="$(curl -fsSL "$api" \
    | grep -o 'https://[^"]*-macos-universal\.dmg' \
    | head -1)"
if [ -z "$dmg_url" ]; then
    echo "Could not find a macOS .dmg asset for '${VERSION}'." >&2
    exit 1
fi

work="$(mktemp -d)"
mount_point=""
cleanup() {
    [ -n "$mount_point" ] && hdiutil detach "$mount_point" -quiet 2>/dev/null || true
    rm -rf "$work"
}
trap cleanup EXIT

dmg="${work}/OpenRig.dmg"
echo "==> Downloading $(basename "$dmg_url")..."
curl -fL# "$dmg_url" -o "$dmg"

echo "==> Mounting..."
mount_point="$(hdiutil attach "$dmg" -nobrowse -readonly \
    | grep -o '/Volumes/.*' | head -1)"
src_app="$(find "$mount_point" -maxdepth 1 -name '*.app' -type d | head -1)"
if [ -z "$src_app" ]; then
    echo "No .app found inside the .dmg." >&2
    exit 1
fi

echo "==> Installing to ${APP_DEST}..."
rm -rf "$APP_DEST"
cp -R "$src_app" "$APP_DEST"

# The whole point: strip the quarantine the download applied so
# Gatekeeper lets the (ad-hoc signed) app run.
echo "==> Removing quarantine..."
xattr -dr com.apple.quarantine "$APP_DEST" 2>/dev/null || true

echo ""
echo "==> Done. Launch with:  open -a OpenRig"
