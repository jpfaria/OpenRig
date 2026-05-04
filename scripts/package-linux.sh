#!/usr/bin/env bash
# Build Linux packages for a given arch and format.
#
# Runs natively on Linux — used by GitHub Actions and by build-linux-local.sh
# (which wraps it in Docker for macOS use).
#
# Usage:
#   ./scripts/package-linux.sh [--arch arm64|x86_64] [--version V] [--format FORMAT] [--output-dir DIR]
#
# Formats:
#   all       — .tar.gz + .deb + .rpm + .AppImage  (default)
#   deb       — .deb only
#   rpm       — .rpm only
#   tarball   — .tar.gz only
#   appimage  — .AppImage only
#
# Output dir: dist/ by default (override with --output-dir)

set -euo pipefail

PROJECT_ROOT="$(cd "$(dirname "$0")/.." && pwd)"
cd "$PROJECT_ROOT"

# ── Defaults ──────────────────────────────────────────────────────────────────
ARCH="$(uname -m)"
VERSION="0.0.0-dev"
FORMAT="all"
OUTPUT_DIR="$PROJECT_ROOT/dist"

# ── Parse args ────────────────────────────────────────────────────────────────
while [ $# -gt 0 ]; do
    case "$1" in
        --arch)       ARCH="$2"; shift 2 ;;
        --version)    VERSION="$2"; shift 2 ;;
        --format)     FORMAT="$2"; shift 2 ;;
        --output-dir) OUTPUT_DIR="$2"; shift 2 ;;
        --help|-h)
            sed -n '/^#/p' "$0" | head -18 | sed 's/^# //'
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
echo "Output:  $OUTPUT_DIR"
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
S="$OUTPUT_DIR/stage"
rm -rf "$S" "$OUTPUT_DIR/AppDir" "$OUTPUT_DIR/appimagetool"
mkdir -p "$OUTPUT_DIR"
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

# Bundle gettext .mo translations under FHS share/. The runtime resolver
# (i18n::resolve_translations_dir) checks <exec_dir>/../share/openrig/
# translations on Linux, which maps to /usr/share/openrig/translations/
# when the binary lives at /usr/bin/openrig.
mkdir -p "$S/usr/share/openrig/translations"
for lang_dir in crates/adapter-gui/translations/*/LC_MESSAGES; do
    if [ -d "$lang_dir" ]; then
        lang="$(basename "$(dirname "$lang_dir")")"
        mkdir -p "$S/usr/share/openrig/translations/$lang/LC_MESSAGES"
        cp "$lang_dir"/*.mo "$S/usr/share/openrig/translations/$lang/LC_MESSAGES/" 2>/dev/null || true
    fi
done

# ── 3. Package ────────────────────────────────────────────────────────────────
echo ""
echo "══════════════════════════════════════════"
echo "  3/3  Creating packages"
echo "══════════════════════════════════════════"

if $BUILD_TARBALL; then
    D="openrig-${VERSION}-linux-${ARCH}"
    mkdir -p "$OUTPUT_DIR/${D}"
    cp  "$S/usr/bin/openrig"                    "$OUTPUT_DIR/${D}/openrig"
    cp -r "$S/usr/lib/openrig/libs"             "$OUTPUT_DIR/${D}/libs"
    cp -r "$S/usr/share/openrig/data"           "$OUTPUT_DIR/${D}/data"
    cp -r "$S/usr/share/openrig/assets"         "$OUTPUT_DIR/${D}/assets"
    cp -r "$S/usr/share/openrig/captures"       "$OUTPUT_DIR/${D}/captures"
    cp -r "$S/usr/share/openrig/translations"   "$OUTPUT_DIR/${D}/translations"
    tar -czf "$OUTPUT_DIR/${D}.tar.gz" -C "$OUTPUT_DIR" "${D}"
    echo "  → $OUTPUT_DIR/${D}.tar.gz"
fi

if $BUILD_DEB; then
    fpm -s dir -t deb --force \
        -n openrig -v "${VERSION}" \
        --architecture "${DEB_ARCH}" \
        --description "OpenRig virtual guitar pedalboard" \
        --url "https://github.com/jpfaria/OpenRig" \
        --maintainer "Joao Paulo Faria" \
        --category sound \
        --depends libasound2 \
        --deb-no-default-config-files \
        -C "$OUTPUT_DIR/stage" \
        --package "$OUTPUT_DIR/openrig_${VERSION}_${DEB_ARCH}.deb" \
        usr
    echo "  → $OUTPUT_DIR/openrig_${VERSION}_${DEB_ARCH}.deb"
fi

if $BUILD_RPM; then
    fpm -s dir -t rpm --force \
        -n openrig -v "${VERSION}" \
        --architecture "${RPM_ARCH}" \
        --description "OpenRig virtual guitar pedalboard" \
        --url "https://github.com/jpfaria/OpenRig" \
        --maintainer "Joao Paulo Faria" \
        --category "Applications/Multimedia" \
        -C "$OUTPUT_DIR/stage" \
        --package "$OUTPUT_DIR/openrig-${VERSION}-1.${RPM_ARCH}.rpm" \
        usr
    echo "  → $OUTPUT_DIR/openrig-${VERSION}-1.${RPM_ARCH}.rpm"
fi

if $BUILD_APPIMAGE; then
    APPIMAGE_ARCH="$ARCH"
    curl -fsSL -o "$OUTPUT_DIR/appimagetool" \
        "https://github.com/AppImage/AppImageKit/releases/download/continuous/appimagetool-${APPIMAGE_ARCH}.AppImage"
    chmod +x "$OUTPUT_DIR/appimagetool"

    APPDIR="$OUTPUT_DIR/AppDir"
    cp -r "$OUTPUT_DIR/stage" "$APPDIR"

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

    APPIMAGE_EXTRACT_AND_RUN=1 ARCH="$APPIMAGE_ARCH" "$OUTPUT_DIR/appimagetool" "$APPDIR" \
        "$OUTPUT_DIR/OpenRig-${VERSION}-linux-${ARCH}.AppImage"
    echo "  → $OUTPUT_DIR/OpenRig-${VERSION}-linux-${ARCH}.AppImage"
fi

echo ""
echo "Done. Packages in $OUTPUT_DIR:"
ls -lh "$OUTPUT_DIR"/openrig* "$OUTPUT_DIR"/OpenRig* 2>/dev/null | awk '{print "  " $NF, $5}' || true
