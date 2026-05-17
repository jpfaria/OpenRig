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
mkdir -p "$S/usr/lib/openrig/libs/nam"
mkdir -p "$S/usr/share/openrig"

cp target/release/adapter-gui              "$S/usr/bin/openrig"
cp -r "libs/nam/linux-${ARCH}"             "$S/usr/lib/openrig/libs/nam/linux-${ARCH}"
cp -r assets                               "$S/usr/share/openrig/assets"

# Desktop integration — staged into $S so it ships in .deb/.rpm/.tar.gz
# (and reused by the AppImage below). Without this the .deb installs no
# launcher/icon and the app only starts from a terminal (issue #475).
# Exec=openrig (bare name): /usr/bin is on PATH for the FHS install, and
# the AppImage's AppRun resolves it too — one .desktop for all formats.
mkdir -p "$S/usr/share/applications" \
         "$S/usr/share/icons/hicolor/scalable/apps" \
         "$S/usr/share/icons/hicolor/256x256/apps"
cat > "$S/usr/share/applications/openrig.desktop" <<'DESKTOP'
[Desktop Entry]
Name=OpenRig
Comment=Virtual guitar pedalboard
Exec=openrig
Icon=openrig
Type=Application
Categories=AudioVideo;Audio;Music;
Terminal=false
DESKTOP
cp crates/adapter-gui/ui/assets/openrig-logomark.svg \
   "$S/usr/share/icons/hicolor/scalable/apps/openrig.svg"
rsvg-convert -w 256 -h 256 \
    crates/adapter-gui/ui/assets/openrig-logomark.svg \
    -o "$S/usr/share/icons/hicolor/256x256/apps/openrig.png"

# The binary links libNeuralAudioCAPI.so with RUNPATH=$ORIGIN, but the lib
# is staged under usr/lib/openrig/libs/nam/, NOT next to the binary — so
# ld.so can't find it and the app dies at startup with "cannot open
# shared object file" (issue #461; macOS solves the equivalent via
# install_name_tool). Point RUNPATH at the staged lib, relative to the
# binary. The path resolves identically once installed to /usr (.deb/
# .rpm), unpacked (.tar.gz), or inside the AppImage's AppDir.
patchelf --set-rpath "\$ORIGIN/../lib/openrig/libs/nam/linux-${ARCH}" \
    "$S/usr/bin/openrig"

# Gate: a package no one can open is worse than a failed build. Verify
# the NAM lib actually resolves through the new RUNPATH before we wrap
# it in a .deb/.AppImage (lesson from #459).
if ldd "$S/usr/bin/openrig" 2>/dev/null \
    | grep -q 'libNeuralAudioCAPI\.so .*not found'; then
    echo "FATAL: libNeuralAudioCAPI.so still unresolved after RUNPATH patch" >&2
    exit 1
fi
echo "    RUNPATH patched; libNeuralAudioCAPI.so resolves"

# Bundled preset library: the 21 default presets under presets/*.yaml ship
# next to plugins/ and assets/ so the app finds them via
# infra_filesystem::detect_data_root().join("presets"). Without this copy,
# a fresh install shows an empty preset list.
if [ -d presets ]; then
    cp -r presets                          "$S/usr/share/openrig/presets"
fi

# Bundled plugins ship as a pre-extracted directory under
# /usr/share/openrig/plugins, which is what
# infra_filesystem::detect_data_root() returns for Linux installs.
# registry::init_many scans this path plus the user-writable root.
if [ -d plugins/source ]; then
    cp -r plugins/source "$S/usr/share/openrig/plugins"
    # Each LV2 plugin carries platform/{linux-x86_64,linux-aarch64,
    # macos-*,windows-*} binaries — Linux package só carrega .so do
    # arch alvo. Drop o que sobra (issue #425):
    #   - sempre: macos-*, windows-*
    #   - x86_64 build: também linux-aarch64
    #   - aarch64 build: também linux-x86_64
    dropped_dirs=0
    drop_patterns=("macos-*" "windows-*")
    if [ "$ARCH" = "x86_64" ]; then
        drop_patterns+=("linux-aarch64")
    elif [ "$ARCH" = "aarch64" ]; then
        drop_patterns+=("linux-x86_64")
    fi
    for pattern in "${drop_patterns[@]}"; do
        while IFS= read -r dir; do
            rm -rf "$dir"
            dropped_dirs=$((dropped_dirs + 1))
        done < <(find "$S/usr/share/openrig/plugins" -type d -path "*/platform/$pattern" 2>/dev/null)
    done
    PLUGIN_COUNT=$(find "$S/usr/share/openrig/plugins" -name 'manifest.yaml' | wc -l | tr -d ' ')
    echo "    bundled plugins ($PLUGIN_COUNT package(s)); dropped $dropped_dirs non-${ARCH} platform dirs"
else
    echo "WARN: plugins/source/ not found — run OpenRig-plugins's pack_plugins or check out the plugin tree first"
fi

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
    cp -r "$S/usr/share/openrig/assets"         "$OUTPUT_DIR/${D}/assets"
    cp -r "$S/usr/share/openrig/translations"   "$OUTPUT_DIR/${D}/translations"
    [ -d "$S/usr/share/openrig/plugins" ] && \
        cp -r "$S/usr/share/openrig/plugins"   "$OUTPUT_DIR/${D}/plugins"
    [ -d "$S/usr/share/openrig/presets" ] && \
        cp -r "$S/usr/share/openrig/presets"   "$OUTPUT_DIR/${D}/presets"
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
        --depends libseat1 \
        --depends alsa-utils \
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
        --depends libseat \
        --depends alsa-utils \
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

    # An AppImage must be self-contained. libseat.so.1 is NEEDED by the
    # binary (Slint/winit backend) but is absent on minimal desktops
    # (no compositor/seatd) — bundle it (issue #461). The build runner
    # links against it, so it is present here to copy.
    mkdir -p "$APPDIR/usr/lib/openrig/syslibs"
    seat_lib="$(ldconfig -p | awk '/libseat\.so\.1/ {print $NF; exit}')"
    if [ -z "$seat_lib" ] || [ ! -e "$seat_lib" ]; then
        echo "FATAL: libseat.so.1 not found on build host — cannot bundle" >&2
        exit 1
    fi
    cp -L "$seat_lib" "$APPDIR/usr/lib/openrig/syslibs/libseat.so.1"

    printf '%s\n' \
        '#!/bin/bash' \
        'HERE="$(dirname "$(readlink -f "${0}")")"' \
        'export OPENRIG_LIBS_DIR="$HERE/usr/lib/openrig/libs"' \
        'export OPENRIG_ASSETS_DIR="$HERE/usr/share/openrig/assets"' \
        'LD_LIBRARY_PATH="$HERE/usr/lib/openrig/syslibs${LD_LIBRARY_PATH:+:$LD_LIBRARY_PATH}"' \
        'for d in "$HERE"/usr/lib/openrig/libs/nam/linux-*; do' \
        '    [ -d "$d" ] && LD_LIBRARY_PATH="$d:$LD_LIBRARY_PATH"' \
        'done' \
        'export LD_LIBRARY_PATH' \
        'exec "$HERE/usr/bin/openrig" "$@"' \
        > "$APPDIR/AppRun"
    chmod +x "$APPDIR/AppRun"

    # appimagetool wants the .desktop + icon at the AppDir root. Reuse
    # the ones already staged under usr/share (AppDir is a copy of the
    # stage tree) — single source, no divergence (issue #475).
    cp "$APPDIR/usr/share/applications/openrig.desktop" "$APPDIR/openrig.desktop"
    cp "$APPDIR/usr/share/icons/hicolor/256x256/apps/openrig.png" "$APPDIR/openrig.png"

    APPIMAGE_EXTRACT_AND_RUN=1 ARCH="$APPIMAGE_ARCH" "$OUTPUT_DIR/appimagetool" "$APPDIR" \
        "$OUTPUT_DIR/OpenRig-${VERSION}-linux-${ARCH}.AppImage"
    echo "  → $OUTPUT_DIR/OpenRig-${VERSION}-linux-${ARCH}.AppImage"
fi

echo ""
echo "Done. Packages in $OUTPUT_DIR:"
ls -lh "$OUTPUT_DIR"/openrig* "$OUTPUT_DIR"/OpenRig* 2>/dev/null | awk '{print "  " $NF, $5}' || true
