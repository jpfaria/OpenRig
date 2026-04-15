#!/bin/bash
# Armbian customize-image.sh hook.
# Runs INSIDE the image chroot after base packages are installed.
# /tmp/overlay/ contains the contents of orange-pi/rootfs/ plus the staged binary and libs.
set -euo pipefail

RELEASE="$1"   # e.g. "bookworm"

echo ">>> [OpenRig] Customizing image for release: $RELEASE"

# ── 1. Install runtime dependencies ──────────────────────────────────────────
echo "jackd2 jackd/tweak_rt_limits boolean true" | debconf-set-selections
DEBIAN_FRONTEND=noninteractive apt-get update -qq
DEBIAN_FRONTEND=noninteractive apt-get install -y --no-install-recommends \
    alsa-utils \
    jackd2 \
    libfreetype6 \
    libfontconfig1 \
    libdrm2 \
    libseat1 \
    libgles2 \
    libgl1-mesa-dri \
    weston \
    adwaita-icon-theme \
    plymouth \
    librsvg2-bin \
    udev \
    locales \
    tzdata \
    keyboard-configuration \
    console-setup

# ── 2. Copy rootfs overlay (etc, usr) ────────────────────────────────────────
echo ">>> [OpenRig] Copying rootfs overlay (systemd, plymouth, helpers)..."
cp -r /tmp/overlay/etc /
cp -r /tmp/overlay/usr /

# ── 3. Install OpenRig from the staged .deb ──────────────────────────────────
echo ">>> [OpenRig] Installing openrig.deb..."
DEBIAN_FRONTEND=noninteractive apt-get install -y --no-install-recommends \
    /tmp/overlay/openrig.deb

if [ ! -e /usr/share/openrig/libs ]; then
    ln -sf /usr/lib/openrig/libs /usr/share/openrig/libs
fi

rm -rf /var/lib/apt/lists/*

# ── 5. Convert OpenRig logo SVGs → PNGs for Plymouth ────────────────────────
echo ">>> [OpenRig] Converting logo assets to PNG..."
rsvg-convert \
    -w 256 -h 256 \
    /tmp/overlay/openrig-logomark.svg \
    -o /usr/share/plymouth/themes/openrig/logo.png
rsvg-convert \
    -w 400 \
    /tmp/overlay/openrig-logotype.svg \
    -o /usr/share/plymouth/themes/openrig/logotype.png

# ── 6. Register and activate Plymouth theme ──────────────────────────────────
echo ">>> [OpenRig] Activating Plymouth theme..."
update-alternatives --install \
    /usr/share/plymouth/themes/default.plymouth \
    default.plymouth \
    /usr/share/plymouth/themes/openrig/openrig.plymouth \
    100
# Load simpledrm in the initramfs so Plymouth has a DRM device to render on.
# simpledrm provides a DRM interface over the U-Boot framebuffer — without it
# Plymouth falls back to text mode and the logo never appears.
# Must be set BEFORE update-alternatives so the initramfs rebuild triggered
# by the alternatives change bakes simpledrm in.
echo 'simpledrm' >> /etc/initramfs-tools/modules

update-alternatives --set \
    default.plymouth \
    /usr/share/plymouth/themes/openrig/openrig.plymouth

# Rebuild initramfs so Plymouth theme + simpledrm are baked in for early boot.
update-initramfs -u -k all

# ── 7. Create users with fixed passwords ─────────────────────────────────────
echo ">>> [OpenRig] Creating openrig user and setting passwords..."
for g in audio video tty input render plugdev dialout; do
    groupadd -f "$g"
done
if ! id openrig >/dev/null 2>&1; then
    useradd --create-home \
        --groups audio,video,tty,input,render,plugdev,dialout \
        --shell /bin/bash \
        openrig
fi
chown -R openrig:openrig /home/openrig
echo 'openrig:openrig' | chpasswd
echo 'root:root'       | chpasswd

# Pre-configure audio device settings for TEYUN Q26 via JACK.
# Both input and output point to jack:system so OpenRig uses a single
# clock and channels are selectable from first boot.
mkdir -p /root/.config/OpenRig
cat > /root/.config/OpenRig/gui-settings.yaml << 'GUI_SETTINGS'
input_devices:
- device_id: jack:system
  name: TEYUN Q26 (JACK)
  sample_rate: 48000
  buffer_size_frames: 64
  bit_depth: 32
output_devices:
- device_id: jack:system
  name: TEYUN Q26 (JACK)
  sample_rate: 48000
  buffer_size_frames: 64
  bit_depth: 32
GUI_SETTINGS

# Create the default project config that openrig.service uses.
cat > /etc/openrig.yaml << 'DEFAULT_PROJECT'
version: 1
name: My Rig
chains:
  - description: guitar 1
    instrument: electric_guitar
    blocks:
      - type: input
        model: standard
        enabled: true
        entries:
          - device_id: "jack:system"
            mode: mono
            channels: [0]
      - type: output
        model: standard
        enabled: true
        entries:
          - device_id: "jack:system"
            mode: stereo
            channels: [0, 1]
DEFAULT_PROJECT

# ── 8. Locale, keyboard, timezone ────────────────────────────────────────────
echo ">>> [OpenRig] Configuring locale (en_US.UTF-8), keyboard (br-abnt2), timezone (America/Sao_Paulo)..."

sed -i 's/^# *en_US.UTF-8 UTF-8/en_US.UTF-8 UTF-8/' /etc/locale.gen
locale-gen en_US.UTF-8
update-locale LANG=en_US.UTF-8 LC_ALL=en_US.UTF-8 LANGUAGE=en_US:en

cat > /etc/default/keyboard <<'EOF'
XKBMODEL="abnt2"
XKBLAYOUT="br"
XKBVARIANT="abnt2"
XKBOPTIONS=""
BACKSPACE="guess"
EOF

ln -sf /usr/share/zoneinfo/America/Sao_Paulo /etc/localtime
echo 'America/Sao_Paulo' > /etc/timezone
dpkg-reconfigure -f noninteractive tzdata || true

# ── 8b. Audio RT capabilities ────────────────────────────────────────────────
echo ">>> [OpenRig] Setting JACK RT capabilities and audio group membership..."
setcap cap_sys_nice,cap_ipc_lock=ep /usr/bin/jackd || true
usermod -aG audio root

# ── 9. Enable systemd services ───────────────────────────────────────────────
echo ">>> [OpenRig] Enabling services, masking jackd..."
systemctl disable jackd.service  2>/dev/null || true
systemctl mask    jackd.service  2>/dev/null || true
systemctl enable weston.service
systemctl enable openrig.service
systemctl enable openrig-irq-affinity.service
systemctl enable openrig-audio-watchdog.service

# ── 9a. Plymouth quit failsafe ────────────────────────────────────────────────
echo ">>> [OpenRig] Capping plymouth-quit-wait timeout at 10s..."
mkdir -p /etc/systemd/system/plymouth-quit-wait.service.d/
cat > /etc/systemd/system/plymouth-quit-wait.service.d/openrig-timeout.conf << 'EOF'
[Service]
TimeoutStartSec=10
EOF

# ── 10. Set permissions on install script ────────────────────────────────────
chmod 755 /usr/local/bin/openrig-install-to-emmc
chmod 755 /usr/local/bin/openrig-reset-audio
chmod 755 /usr/local/bin/openrig-audio-watchdog

# ── 10a. USB-C TCPM workaround (RK3588 USB-C port stability) ─────────────────
echo ">>> [OpenRig] Installing USB-C host-mode DTB overlay (Scarlett stability)..."
if command -v armbian-add-overlay >/dev/null 2>&1; then
    armbian-add-overlay /tmp/overlay/openrig-usbc-host.dts || \
        echo ">>> [OpenRig] WARNING: armbian-add-overlay failed, skipping USB-C overlay"
else
    echo ">>> [OpenRig] armbian-add-overlay missing, falling back to manual dtc"
    mkdir -p /boot/overlay-user
    dtc -I dts -O dtb -o /boot/overlay-user/openrig-usbc-host.dtbo \
        /tmp/overlay/openrig-usbc-host.dts || \
        echo ">>> [OpenRig] WARNING: dtc failed, skipping USB-C overlay"
    if grep -q '^user_overlays=' /boot/armbianEnv.txt 2>/dev/null; then
        sed -i 's|^user_overlays=.*|&  openrig-usbc-host|' /boot/armbianEnv.txt || true
    else
        echo 'user_overlays=openrig-usbc-host' >> /boot/armbianEnv.txt || true
    fi
fi

# ── 11. Silent kiosk boot ────────────────────────────────────────────────────
echo ">>> [OpenRig] Configuring silent kiosk boot..."

KERNEL_ARGS='quiet splash loglevel=3 rd.systemd.show_status=false rd.udev.log_level=3'
if grep -q "^extraargs=" /boot/armbianEnv.txt 2>/dev/null; then
    sed -i "s|^extraargs=.*|extraargs=${KERNEL_ARGS}|" /boot/armbianEnv.txt
else
    echo "extraargs=${KERNEL_ARGS}" >> /boot/armbianEnv.txt
fi

sed -i 's/^verbosity=.*/verbosity=0/' /boot/armbianEnv.txt 2>/dev/null || true
sed -i 's/^console=.*/console=serial/' /boot/armbianEnv.txt 2>/dev/null || true
sed -i 's/^bootlogo=.*/bootlogo=true/' /boot/armbianEnv.txt 2>/dev/null || true

# Armbian first-run wizard and filesystem resize.
rm -f /root/.not_logged_in_yet
touch /root/.no_rootfs_resize_at_firstboot
systemctl disable armbian-firstrun.service              2>/dev/null || true
systemctl disable armbian-firstrun-config.service       2>/dev/null || true
systemctl disable armbian-resize-filesystem.service     2>/dev/null || true
systemctl mask    armbian-firstrun.service              2>/dev/null || true
systemctl mask    armbian-firstrun-config.service       2>/dev/null || true
systemctl mask    armbian-resize-filesystem.service     2>/dev/null || true
rm -f /etc/profile.d/armbian-check-first-run.sh
rm -f /etc/update-motd.d/30-armbian-sysinfo             2>/dev/null || true

# Mask gettys so no text login prompt appears on any tty.
for tty in 1 2 3 4 5 6; do
    systemctl disable "getty@tty${tty}.service" 2>/dev/null || true
    systemctl mask    "getty@tty${tty}.service" 2>/dev/null || true
done
systemctl disable serial-getty@ttyS0.service  2>/dev/null || true
systemctl mask    serial-getty@ttyS0.service  2>/dev/null || true

echo ">>> [OpenRig] Image customization complete."
