#!/bin/bash
# Local macOS packaging script — builds and creates .app + .dmg for testing
# Usage: ./scripts/package-macos.sh [version]
set -euo pipefail

VERSION="${1:-dev}"
REPO_ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
cd "$REPO_ROOT"

echo "==> Building (native, no universal)..."
cargo build --release -p adapter-gui

echo "==> Creating .app bundle..."
APP="dist/OpenRig.app"
rm -rf "$APP" dist/dmg_contents dist/OpenRig-*.dmg
mkdir -p "$APP/Contents/MacOS"
mkdir -p "$APP/Contents/Resources/libs/lv2"
mkdir -p "$APP/Contents/Resources/libs/nam"

cp target/release/adapter-gui "$APP/Contents/MacOS/openrig"
chmod +x "$APP/Contents/MacOS/openrig"

cp -r libs/lv2/macos-universal "$APP/Contents/Resources/libs/lv2/macos-universal"
cp -r libs/nam/macos-universal "$APP/Contents/Resources/libs/nam/macos-universal"
cp -r data/lv2                 "$APP/Contents/Resources/data"
cp -r assets                   "$APP/Contents/Resources/assets"

# Generate placeholder .icns if none exists
ICNS_SRC="assets/brands/openrig/icon.icns"
if [ ! -f "$ICNS_SRC" ]; then
    echo "==> Generating placeholder icon..."
    TMP_ICONSET=$(mktemp -d)/openrig.iconset
    mkdir -p "$TMP_ICONSET"
    for SIZE in 16 32 64 128 256 512; do
        python3 -c "
import struct, zlib, base64, sys
size = $SIZE
# Minimal PNG: dark background with 'OR' text approximation
def png_chunk(tag, data):
    c = zlib.crc32(tag + data) & 0xffffffff
    return struct.pack('>I', len(data)) + tag + data + struct.pack('>I', c)
w, h = size, size
raw = b''
for y in range(h):
    raw += b'\x00'
    for x in range(w):
        # dark blue bg
        r, g, b = 0x1a, 0x1a, 0x2e
        # white cross as simple icon indicator
        if abs(x - w//2) < w//8 or abs(y - h//2) < h//8:
            r, g, b = 0xff, 0xff, 0xff
        raw += bytes([r, g, b, 0xff])
compressed = zlib.compress(raw)
png = b'\x89PNG\r\n\x1a\n'
png += png_chunk(b'IHDR', struct.pack('>IIBBBBB', w, h, 8, 2, 0, 0, 0))
png += png_chunk(b'IDAT', compressed)
png += png_chunk(b'IEND', b'')
sys.stdout.buffer.write(png)
" > "$TMP_ICONSET/icon_${SIZE}x${SIZE}.png"
        cp "$TMP_ICONSET/icon_${SIZE}x${SIZE}.png" "$TMP_ICONSET/icon_${SIZE}x${SIZE}@2x.png" 2>/dev/null || true
    done
    iconutil -c icns "$TMP_ICONSET" -o "assets/brands/openrig/icon.icns"
    ICNS_SRC="assets/brands/openrig/icon.icns"
fi

cp "$ICNS_SRC" "$APP/Contents/Resources/openrig.icns"

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

echo "==> Creating .dmg with drag-to-Applications..."
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
echo "    Open with: open dist/OpenRig-${VERSION}-macos.dmg"
