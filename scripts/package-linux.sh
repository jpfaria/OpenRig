#!/usr/bin/env bash
# Build Linux packages for a given arch and format.
#
# Runs natively on Linux — used by GitHub Actions and by build-linux-local.sh
# (which wraps it in Docker for macOS use).
#
# Usage:
#   ./scripts/package-linux.sh [--arch arm64|x86_64] [--version V] [--format FORMAT]
#
# Formats:
#   all       — .tar.gz + .deb + .rpm + .AppImage  (default)
#   deb       — .deb only
#   rpm       — .rpm only
#   tarball   — .tar.gz only
#   appimage  — .AppImage only
#
# Output (in dist/):
#   openrig-{VERSION}-linux-{arch}.tar.gz   (tarball, all)
#   openrig_{VERSION}_{deb_arch}.deb        (deb, all)
#   openrig-{VERSION}-1.{rpm_arch}.rpm      (rpm, all)
#   OpenRig-{VERSION}-linux-{arch}.AppImage (appimage, all)

set -euo pipefail

PROJECT_ROOT="$(cd "$(dirname "$0")/.." && pwd)"
cd "$PROJECT_ROOT"

# ── Defaults ──────────────────────────────────────────────────────────────────
ARCH="$(uname -m)"   # auto-detect native arch
VERSION="0.0.0-dev"
FORMAT="all"

# ── Parse args ────────────────────────────────────────────────────────────────
while [ $# -gt 0 ]; do
    case "$1" in
        --arch)     ARCH="$2"; shift 2 ;;
        --version)  VERSION="$2"; shift 2 ;;
        --format)   FORMAT="$2"; shift 2 ;;
        --help|-h)
            sed -n '/^#/p' "$0" | head -22 | sed 's/^# //'
            exit 0
            ;;
        *) echo "Unknown argument: $1"; exit 1 ;;
    esac
done

# Normalise arch → uname -m form
case "$ARCH" in
    arm64|aarch64)       ARCH="aarch64"; DEB_ARCH="arm64"; RPM_ARCH="aarch64" ;;
    x86_64|amd64|x64)   ARCH="x86_64";  DEB_ARCH="amd64"; RPM_ARCH="x86_64"  ;;
    *) echo "Unsupported arch: $ARCH"; exit 1 ;;
esac

# Resolve format flags
BUILD_DEB=false
BUILD_RPM=false
BUILD_TARBALL=false
BUILD_APPIMAGE=false
case "$FORMAT" in
    all)      BUILD_DEB=true; BUILD_RPM=true; BUILD_TARBALL=true; BUILD_APPIMAGE=true ;;
    deb)      BUILD_DEB=true ;;
    rpm)      BUILD_RPM=true ;;
    tarball)  BUILD_TARBALL=true ;;
    appimage) BUILD_APPIMAGE=true ;;
    *) echo "Unknown format: $FORMAT (use deb|rpm|tarball|appimage|all)"; exit 1 ;;
esac

echo "OpenRig — Linux package builder"
echo "Arch:    $ARCH"
echo "Version: $VERSION"
echo "Format:  $FORMAT"
echo ""

# ── 1. Build ──────────────────────────────────────────────────────────────────
echo "══════════════════════════════════════════"
echo "  1/3  cargo build --release"
echo "══════════════════════════════════════════"
cargo build --release -p adapter-gui

# ── 2. Stage install tree ─────────────────────────────────────────────────────
echo ""
echo "══════════════════════════════════════════"
echo "  2/3  Staging install tree"
echo "══════════════════════════════════════════"
S=dist/stage
rm -rf dist
mkdir -p "$S/usr/bin"
mkdir -p "$S/usr/lib/openrig/libs/lv2"
mkdir -p "$S/usr/lib/openrig/libs/nam"
mkdir -p "$S/usr/share/openrig/data"

cp target/release/adapter-gui              "$S/usr/bin/openrig"
cp -r "libs/lv2/linux-${ARCH}"             "$S/usr/lib/openrig/libs/lv2/linux-${ARCH}"
cp -r "libs/nam/linux-${ARCH}"             "$S/usr/lib/openrig/libs/nam/linux-${ARCH}"
cp -r data/lv2                             "$S/usr/share/openrig/data/lv2"
cp -r assets                               "$S/usr/share/openrig/assets"
cp -r captures                             "$S/usr/share/openrig/captures"

# ── 3. Package ────────────────────────────────────────────────────────────────
echo ""
echo "══════════════════════════════════════════"
echo "  3/3  Creating packages"
echo "══════════════════════════════════════════"

if $BUILD_TARBALL; then
    D="openrig-${VERSION}-linux-${ARCH}"
    mkdir -p "dist/${D}"
    cp  "$S/usr/bin/openrig"                    "dist/${D}/openrig"
    cp -r "$S/usr/lib/openrig/libs"             "dist/${D}/libs"
    cp -r "$S/usr/share/openrig/data"           "dist/${D}/data"
    cp -r "$S/usr/share/openrig/assets"         "dist/${D}/assets"
    cp -r "$S/usr/share/openrig/captures"       "dist/${D}/captures"
    tar -czf "dist/${D}.tar.gz" -C dist "${D}"
    echo "  → dist/${D}.tar.gz"
fi

if $BUILD_DEB; then
    fpm -s dir -t deb \
        -n openrig -v "${VERSION}" \
        --architecture "${DEB_ARCH}" \
        --description "OpenRig virtual guitar pedalboard" \
        --url "https://github.com/jpfaria/OpenRig" \
        --maintainer "Joao Paulo Faria" \
        --category sound \
        --depends libasound2 \
        --deb-no-default-config-files \
        -C dist/stage \
        --package "dist/openrig_${VERSION}_${DEB_ARCH}.deb" \
        usr
    echo "  → dist/openrig_${VERSION}_${DEB_ARCH}.deb"
fi

if $BUILD_RPM; then
    fpm -s dir -t rpm \
        -n openrig -v "${VERSION}" \
        --architecture "${RPM_ARCH}" \
        --description "OpenRig virtual guitar pedalboard" \
        --url "https://github.com/jpfaria/OpenRig" \
        --maintainer "Joao Paulo Faria" \
        --category "Applications/Multimedia" \
        -C dist/stage \
        --package "dist/openrig-${VERSION}-1.${RPM_ARCH}.rpm" \
        usr
    echo "  → dist/openrig-${VERSION}-1.${RPM_ARCH}.rpm"
fi

if $BUILD_APPIMAGE; then
    APPIMAGE_ARCH="$ARCH"
    curl -fsSL -o dist/appimagetool \
        "https://github.com/AppImage/AppImageKit/releases/download/continuous/appimagetool-${APPIMAGE_ARCH}.AppImage"
    chmod +x dist/appimagetool

    APPDIR=dist/AppDir
    cp -r dist/stage "$APPDIR"

    printf '%s\n' \
        '#!/bin/bash' \
        'HERE="$(dirname "$(readlink -f "${0}")")"' \
        'export OPENRIG_LIBS_DIR="$HERE/usr/lib/openrig/libs"' \
        'export OPENRIG_DATA_DIR="$HERE/usr/share/openrig/data"' \
        'export OPENRIG_ASSETS_DIR="$HERE/usr/share/openrig/assets"' \
        'export OPENRIG_CAPTURES_DIR="$HERE/usr/share/openrig/captures"' \
        'exec "$HERE/usr/bin/openrig" "$@"' \
        > "$APPDIR/AppRun"
    chmod +x "$APPDIR/AppRun"

    printf '%s\n' \
        '[Desktop Entry]' \
        'Name=OpenRig' \
        'Exec=openrig' \
        'Icon=openrig' \
        'Type=Application' \
        'Categories=Audio;Music;' \
        > "$APPDIR/openrig.desktop"

    rsvg-convert -w 256 -h 256 \
        crates/adapter-gui/ui/assets/openrig-logomark.svg \
        -o "$APPDIR/openrig.png"

    APPIMAGE_EXTRACT_AND_RUN=1 ARCH="$APPIMAGE_ARCH" dist/appimagetool "$APPDIR" \
        "dist/OpenRig-${VERSION}-linux-${ARCH}.AppImage"
    echo "  → dist/OpenRig-${VERSION}-linux-${ARCH}.AppImage"
fi

echo ""
echo "Done. Packages in dist/:"
ls -lh dist/openrig* dist/OpenRig* 2>/dev/null | awk '{print "  " $NF, $5}'
