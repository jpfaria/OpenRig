#!/bin/bash
# Armbian customize-image.sh hook.
# Runs INSIDE the image chroot after base packages are installed.
# /tmp/overlay/ contains the contents of orange-pi/rootfs/ plus the staged binary and libs.
set -euo pipefail

RELEASE="$1"   # e.g. "bookworm"

echo ">>> [OpenRig] Customizing image for release: $RELEASE"

# ── 1. Install runtime dependencies ──────────────────────────────────────────
# Pre-answer jackd2 debconf question (realtime privileges) to avoid interactive prompt
echo "jackd2 jackd/tweak_rt_limits boolean true" | debconf-set-selections
DEBIAN_FRONTEND=noninteractive apt-get update -qq
DEBIAN_FRONTEND=noninteractive apt-get install -y --no-install-recommends \
    alsa-utils \
    jackd2 \
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

# ── 3. Install OpenRig binary, libs, data and assets ─────────────────────────
echo ">>> [OpenRig] Installing OpenRig from release..."
RELEASE_DIR=/tmp/overlay/openrig-release

install -m 755 "$RELEASE_DIR/openrig" /usr/local/bin/openrig

mkdir -p /usr/local/share/openrig
cp -r "$RELEASE_DIR/libs"     /usr/local/share/openrig/
cp -r "$RELEASE_DIR/data"     /usr/local/share/openrig/
cp -r "$RELEASE_DIR/assets"   /usr/local/share/openrig/
cp -r "$RELEASE_DIR/captures" /usr/local/share/openrig/ 2>/dev/null || true

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

# ── 9. Enable systemd services ───────────────────────────────────────────────
echo ">>> [OpenRig] Enabling jackd.service and openrig.service..."
systemctl enable jackd.service
systemctl enable openrig.service

# ── 10. Set permissions on install script ────────────────────────────────────
chmod 755 /usr/local/bin/openrig-install-to-emmc

# ── 11. Silent kiosk boot ─────────────────────────────────────────────────────
echo ">>> [OpenRig] Configuring silent kiosk boot..."

# Quiet kernel boot — suppress console messages, keep Plymouth splash
grep -q "^extraargs=" /boot/armbianEnv.txt 2>/dev/null \
    && sed -i 's/^extraargs=.*/extraargs=quiet splash loglevel=3 rd.systemd.show_status=false rd.udev.log_level=3/' /boot/armbianEnv.txt \
    || echo "extraargs=quiet splash loglevel=3 rd.systemd.show_status=false rd.udev.log_level=3" >> /boot/armbianEnv.txt

# Disable Armbian first-run configuration wizard
systemctl disable armbian-firstrun 2>/dev/null || true
systemctl disable armbian-firstrun-config 2>/dev/null || true
rm -f /etc/profile.d/armbian-check-first-run.sh

# Disable login prompt on tty1 (OpenRig takes over the display via linuxkms)
systemctl disable getty@tty1.service 2>/dev/null || true
systemctl mask getty@tty1.service 2>/dev/null || true

echo ">>> [OpenRig] Image customization complete."
