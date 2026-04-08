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
mkdir -p "$APP/Contents/Resources/libs/lv2"
mkdir -p "$APP/Contents/Resources/libs/nam"

echo "==> Creating universal binary with lipo..."
lipo -create \
    target/aarch64-apple-darwin/release/adapter-gui \
    target/x86_64-apple-darwin/release/adapter-gui \
    -output "$APP/Contents/MacOS/openrig"
chmod +x "$APP/Contents/MacOS/openrig"

# Copy dylib into Frameworks (so the OS can find it at runtime)
cp libs/nam/macos-universal/libNeuralAudioCAPI.dylib "$APP/Contents/Frameworks/"

# Fix rpath so binary finds the dylib inside the bundle
install_name_tool \
    -add_rpath "@executable_path/../Frameworks" \
    "$APP/Contents/MacOS/openrig" 2>/dev/null || true

cp assets/brands/openrig/icon.icns "$APP/Contents/Resources/openrig.icns"
cp -r libs/lv2/macos-universal "$APP/Contents/Resources/libs/lv2/macos-universal"
cp -r libs/nam/macos-universal "$APP/Contents/Resources/libs/nam/macos-universal"
cp -r data/lv2                 "$APP/Contents/Resources/data"
cp -r assets                   "$APP/Contents/Resources/assets"

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

# ── 5. Ad-hoc signing (bypasses Gatekeeper without Apple certificate) ─────────
echo "==> Signing app (ad-hoc)..."
codesign --force --deep --sign - "$APP"
xattr -cr "$APP"

# ── 6. Quick smoke test ────────────────────────────────────────────────────────
echo "==> Testing binary launches..."
timeout 3 "$APP/Contents/MacOS/openrig" --help 2>&1 || true
echo "    (timeout is expected if app opens a window)"

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
    "dist/OpenRig-${VERSION}-macos.dmg"

rm -rf dist/dmg_contents

echo ""
echo "==> Done: dist/OpenRig-${VERSION}-macos.dmg"
echo ""
echo "Para abrir:"
echo "  open dist/OpenRig-${VERSION}-macos.dmg"
