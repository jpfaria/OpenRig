#!/bin/bash
# Mirrors exactly what GitHub Actions does for the macOS build.
# Usage: ./scripts/package-macos.sh [version]
set -euo pipefail

VERSION="${1:-dev}"
REPO_ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
cd "$REPO_ROOT"

# ── 1. Rust targets ──────────────────────────────────────────────────────────
echo "==> Adding Rust targets..."
rustup target add aarch64-apple-darwin x86_64-apple-darwin

# ── 2. Build (universal: arm64 + x86_64) ─────────────────────────────────────
echo "==> Building arm64..."
cargo build --release --target aarch64-apple-darwin -p adapter-gui

echo "==> Building x86_64..."
cargo build --release --target x86_64-apple-darwin -p adapter-gui

# ── 3. Generate .icns from OpenRig logo SVG ───────────────────────────────────
echo "==> Generating icon from openrig-logomark.svg..."
SVG="crates/adapter-gui/ui/assets/openrig-logomark.svg"
mkdir -p assets/brands/openrig
TMP_ICONSET=$(mktemp -d)/openrig.iconset
mkdir -p "$TMP_ICONSET"

# iconset requires these specific filenames
# @1x sizes: 16, 32, 128, 256, 512
# @2x sizes: 32, 64, 256, 512, 1024 (named as @2x of the @1x size)
for SIZE in 16 32 128 256 512; do
    sips -s format png --resampleHeightWidth $SIZE $SIZE "$SVG" \
        --out "$TMP_ICONSET/icon_${SIZE}x${SIZE}.png" >/dev/null
done
for SIZE in 32 64 256 512 1024; do
    HALF=$((SIZE / 2))
    sips -s format png --resampleHeightWidth $SIZE $SIZE "$SVG" \
        --out "$TMP_ICONSET/icon_${HALF}x${HALF}@2x.png" >/dev/null
done

iconutil -c icns "$TMP_ICONSET" -o assets/brands/openrig/icon.icns
echo "    icon.icns generated ($(du -h assets/brands/openrig/icon.icns | cut -f1))"

# ── 4. Create .app bundle (same as CI) ───────────────────────────────────────
echo "==> Creating .app bundle..."
APP="dist/OpenRig.app"
rm -rf "$APP" dist/dmg_contents dist/OpenRig-*.dmg

mkdir -p "$APP/Contents/MacOS"
mkdir -p "$APP/Contents/Frameworks"
mkdir -p "$APP/Contents/Resources"

echo "==> Creating universal binary with lipo..."
lipo -create \
    target/aarch64-apple-darwin/release/adapter-gui \
    target/x86_64-apple-darwin/release/adapter-gui \
    -output "$APP/Contents/MacOS/openrig"
chmod +x "$APP/Contents/MacOS/openrig"

# ── NAM wrapper dylib ────────────────────────────────────────────────────────
# The nam crate's build.rs cmake-builds cpp/ (NeuralAmpModelerCore) into
# libnam_wrapper.dylib (install_name @rpath/libnam_wrapper.dylib) and links the
# binary against it. There is no committed prebuilt anymore — locate the
# per-arch artifact cargo produced under target/<triple>/release/build/nam-*/
# and lipo the two arches into one universal dylib, mirroring the binary above.
find_nam_wrapper() {
    # newest matching artifact for the given target triple
    find "target/$1/release/build" -path '*/nam-*/out/lib/libnam_wrapper.dylib' \
        2>/dev/null | head -1
}
NAM_ARM="$(find_nam_wrapper aarch64-apple-darwin)"
NAM_X64="$(find_nam_wrapper x86_64-apple-darwin)"
if [ -z "$NAM_ARM" ] || [ -z "$NAM_X64" ]; then
    echo "FATAL: libnam_wrapper.dylib not found for both arches — did cargo build run?" >&2
    echo "  arm64:  ${NAM_ARM:-<missing>}" >&2
    echo "  x86_64: ${NAM_X64:-<missing>}" >&2
    exit 1
fi
lipo -create "$NAM_ARM" "$NAM_X64" -output "$APP/Contents/Frameworks/libnam_wrapper.dylib"
echo "    bundled universal libnam_wrapper.dylib into Frameworks/"

# Fix rpath so binary finds the dylib inside the bundle
install_name_tool \
    -add_rpath "@executable_path/../Frameworks" \
    "$APP/Contents/MacOS/openrig" 2>/dev/null || true

cp assets/brands/openrig/icon.icns "$APP/Contents/Resources/openrig.icns"
cp -r assets                   "$APP/Contents/Resources/assets"

# Bundled preset library: the default presets under presets/*.yaml ship
# next to plugins/ and assets/ so the app finds them via
# infra_filesystem::detect_data_root().join("presets"). Without this copy,
# a fresh install shows an empty preset list.
if [ -d presets ]; then
    cp -r presets              "$APP/Contents/Resources/presets"
fi
# data/lv2, libs/lv2, captures were removed in 2011110d — LV2 plugins now
# ship via openrig-plugins.zip (extracted on first launch).

# Bundle plugins as a pre-extracted directory. plugin_loader::registry::
# init_many scans <.app>/Contents/Resources/plugins (this path) plus the
# user-writable root in parallel. No first-launch extraction step.
# Skip silently in dev when the source tree isn't checked out alongside
# OpenRig — registry::init falls back to the user root only.
if [ -d plugins/source ]; then
    cp -r plugins/source "$APP/Contents/Resources/plugins"
    # Each LV2 plugin carries platform/{linux-*,macos-*,windows-*}
    # binaries — macOS .app só carrega .dylib, então .so/.dll são MB
    # inúteis. Drop tudo que não é macOS (issue #425).
    dropped_dirs=0
    for pattern in "linux-*" "windows-*"; do
        while IFS= read -r dir; do
            rm -rf "$dir"
            dropped_dirs=$((dropped_dirs + 1))
        done < <(find "$APP/Contents/Resources/plugins" -type d -path "*/platform/$pattern" 2>/dev/null)
    done
    PLUGIN_COUNT=$(find "$APP/Contents/Resources/plugins" -name 'manifest.yaml' | wc -l | tr -d ' ')
    echo "    bundled plugins ($PLUGIN_COUNT package(s)); dropped $dropped_dirs non-macOS platform dirs"
else
    echo "    NOTE: plugins/source/ not found — .app ships without bundled plugins"
fi

# Bundle gettext .mo translations. build.rs writes per-locale catalogs
# under crates/adapter-gui/translations/<lang>/LC_MESSAGES/; we mirror
# the layout under Resources/translations so i18n::resolve_translations_dir
# finds them at runtime via the Mac.app/Resources fallback.
mkdir -p "$APP/Contents/Resources/translations"
for lang_dir in crates/adapter-gui/translations/*/LC_MESSAGES; do
    if [ -d "$lang_dir" ]; then
        lang="$(basename "$(dirname "$lang_dir")")"
        mkdir -p "$APP/Contents/Resources/translations/$lang/LC_MESSAGES"
        cp -r "$lang_dir"/*.mo "$APP/Contents/Resources/translations/$lang/LC_MESSAGES/" 2>/dev/null || true
    fi
done

cat > "$APP/Contents/Info.plist" <<PLIST
<?xml version="1.0" encoding="UTF-8"?>
<!DOCTYPE plist PUBLIC "-//Apple//DTD PLIST 1.0//EN"
  "http://www.apple.com/DTDs/PropertyList-1.0.dtd">
<plist version="1.0">
<dict>
  <key>CFBundleExecutable</key><string>openrig</string>
  <key>CFBundleIdentifier</key><string>com.openrig.app</string>
  <key>CFBundleName</key><string>OpenRig</string>
  <key>CFBundleDisplayName</key><string>OpenRig</string>
  <key>CFBundleIconFile</key><string>openrig</string>
  <key>CFBundleVersion</key><string>${VERSION}</string>
  <key>CFBundleShortVersionString</key><string>${VERSION}</string>
  <key>CFBundlePackageType</key><string>APPL</string>
  <key>LSMinimumSystemVersion</key><string>11.0</string>
  <key>NSHighResolutionCapable</key><true/>
  <key>NSMicrophoneUsageDescription</key>
  <string>OpenRig uses the microphone for audio input.</string>
</dict>
</plist>
PLIST

# ── 5. Ad-hoc signing — inside-out so the bundle signature is VALID ──────────
# Ad-hoc signing does NOT bypass Gatekeeper. Its only purpose: a *valid*
# signature downgrades the Gatekeeper block from "OpenRig is damaged"
# (hard, unbypassable) to "unidentified developer" (right-click → Open
# works, no Terminal). `codesign --deep` is unreliable on bundles with
# Mach-O outside Contents/MacOS (we ship dylibs in Frameworks/ and nested
# plugin .dylibs in Resources/plugins/.../macos-universal/) — it leaves
# some unsigned, the seal fails verification, and macOS reports "damaged".
# Fix: sign every Mach-O explicitly, innermost first, bundle last.
# No `xattr -cr` here: the user's download re-applies com.apple.quarantine,
# so stripping it at build time is a no-op (issue #459).
echo "==> Signing app (ad-hoc, inside-out)..."
# NUL-delimited find + `case` on full `file` output. No `grep -q` (it
# closes the pipe early → SIGPIPE on `file`, which under `set -o
# pipefail` aborts the loop while `find` keeps writing → a flood of
# "printf: Broken pipe" and exit 1 once the .app's full plugin tree is
# present). No per-file `-exec sh -c` subshell either (issue #463,
# regression of #459 which only tested a tiny bundle).
# Order matters: codesign of the main executable verifies that every
# subcomponent it links (e.g. Frameworks/libnam_wrapper.dylib) is
# already signed, else it fails "code object is not signed at all / In
# subcomponent: ...". `find` order is not inside-out, so sign every
# nested Mach-O here but SKIP the main executable, then sign it after
# the loop (deps first), then the bundle last (issue #463).
MAIN_EXE="$APP/Contents/MacOS/openrig"
while IFS= read -r -d '' f; do
    [ "$f" = "$MAIN_EXE" ] && continue
    case "$(file -b "$f" 2>/dev/null)" in
        *Mach-O*)
            codesign --force --sign - --timestamp=none "$f" \
                || { echo "FATAL: codesign failed for $f" >&2; exit 1; }
            ;;
    esac
done < <(find "$APP/Contents" -type f -print0)
codesign --force --sign - --timestamp=none "$MAIN_EXE" \
    || { echo "FATAL: codesign failed for $MAIN_EXE" >&2; exit 1; }
codesign --force --sign - --timestamp=none "$APP" \
    || { echo "FATAL: codesign failed for the .app bundle" >&2; exit 1; }

# Gate: an invalid signature IS the "damaged" bug. Fail the build loudly
# here rather than ship a .dmg that no one can open.
echo "==> Verifying signature..."
codesign --verify --deep --strict --verbose=2 "$APP"
echo "    signature valid"

# ── 6. Verify binary ──────────────────────────────────────────────────────────
echo "==> Verifying binary..."
file "$APP/Contents/MacOS/openrig"
echo "    binary OK"

# ── 7. Create .dmg with drag-to-Applications ──────────────────────────────────
echo "==> Creating .dmg..."
mkdir -p dist/dmg_contents
cp -r "$APP" dist/dmg_contents/OpenRig.app
ln -sf /Applications dist/dmg_contents/Applications

hdiutil detach "/Volumes/OpenRig ${VERSION}" 2>/dev/null || true
hdiutil create \
    -volname "OpenRig ${VERSION}" \
    -srcfolder dist/dmg_contents \
    -ov -format UDZO \
    "dist/OpenRig-${VERSION}-macos-universal.dmg"

rm -rf dist/dmg_contents

echo ""
echo "==> Done: dist/OpenRig-${VERSION}-macos-universal.dmg"
echo ""
echo "Para abrir:"
echo "  open dist/OpenRig-${VERSION}-macos-universal.dmg"
