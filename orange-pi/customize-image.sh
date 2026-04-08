#!/bin/bash
# Armbian customize-image.sh hook.
# Runs INSIDE the image chroot after base packages are installed.
# /tmp/overlay/ contains the contents of orange-pi/rootfs/ plus the staged binary and libs.
set -euo pipefail

RELEASE="$1"   # e.g. "bookworm"

echo ">>> [OpenRig] Customizing image for release: $RELEASE"

# ── 1. Install runtime dependencies ──────────────────────────────────────────
apt-get update -qq
apt-get install -y --no-install-recommends \
    alsa-utils \
    libfreetype6 \
    libfontconfig1 \
    libdrm2 \
    libgles2 \
    plymouth \
    librsvg2-bin \
    udev \
    && rm -rf /var/lib/apt/lists/*

# ── 2. Copy rootfs overlay (etc, usr) ────────────────────────────────────────
echo ">>> [OpenRig] Copying rootfs overlay..."
cp -r /tmp/overlay/etc /
cp -r /tmp/overlay/usr /

# ── 3. Install OpenRig binary ─────────────────────────────────────────────────
echo ">>> [OpenRig] Installing OpenRig binary..."
install -m 755 /tmp/overlay/bin/openrig /usr/local/bin/openrig

# ── 4. Install LV2 and NAM libs ──────────────────────────────────────────────
echo ">>> [OpenRig] Installing libs..."
mkdir -p /usr/local/lib/openrig
cp -r /tmp/overlay/lib/lv2 /usr/local/lib/openrig/
cp -r /tmp/overlay/lib/nam /usr/local/lib/openrig/

# ── 5. Convert OpenRig logo SVG → PNG for Plymouth ───────────────────────────
echo ">>> [OpenRig] Converting logo to PNG..."
rsvg-convert \
    -w 256 -h 256 \
    /tmp/overlay/openrig-logomark.svg \
    -o /usr/share/plymouth/themes/openrig/logo.png

# ── 6. Register and activate Plymouth theme ──────────────────────────────────
echo ">>> [OpenRig] Activating Plymouth theme..."
update-alternatives --install \
    /usr/share/plymouth/themes/default.plymouth \
    default.plymouth \
    /usr/share/plymouth/themes/openrig/openrig.plymouth \
    100
update-alternatives --set \
    default.plymouth \
    /usr/share/plymouth/themes/openrig/openrig.plymouth

# ── 7. Create openrig system user ────────────────────────────────────────────
echo ">>> [OpenRig] Creating openrig user..."
groupadd -f video
useradd --system --no-create-home \
    --groups audio,video \
    --shell /usr/sbin/nologin \
    openrig

# ── 8. Add udev rule to pin Teyun Q-26 as card "Q26" ─────────────────────────
# USB Vendor:Product ID for Teyun Q-26: 1852:5065
# If incorrect, verify on first boot with `lsusb` and update the rule.
cat > /etc/udev/rules.d/90-teyun-q26.rules <<'EOF'
# Teyun Q-26 USB Audio Interface — pin as card "Q26"
SUBSYSTEM=="sound", ATTR{id}=="Q26", GOTO="q26_done"
SUBSYSTEM=="sound", SUBSYSTEMS=="usb", \
  ATTRS{idVendor}=="1852", ATTRS{idProduct}=="5065", \
  ATTR{id}="Q26"
LABEL="q26_done"
EOF

# ── 9. Enable systemd service ─────────────────────────────────────────────────
echo ">>> [OpenRig] Enabling openrig.service..."
systemctl enable openrig.service

echo ">>> [OpenRig] Image customization complete."
