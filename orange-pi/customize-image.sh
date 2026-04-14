#!/bin/bash
# Armbian customize-image.sh hook.
# Runs INSIDE the image chroot after base packages are installed.
# /tmp/overlay/ contains the contents of orange-pi/rootfs/ plus the staged binary and libs.
set -euo pipefail

RELEASE="$1"   # e.g. "bookworm"

echo ">>> [OpenRig] Customizing image for release: $RELEASE"

# ── 1. Install runtime dependencies ──────────────────────────────────────────
# System-level packages that are NOT pulled in by the openrig .deb itself.
# Plymouth is intentionally excluded for now — boot is kept verbose/text so
# issues can be diagnosed via HDMI. Re-add plymouth + librsvg2-bin and
# re-enable steps 5/6 once the kiosk boot sequence is validated.
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
    udev \
    locales \
    tzdata \
    keyboard-configuration \
    console-setup

# ── 2. Copy rootfs overlay (etc, usr) ────────────────────────────────────────
# This overlay only contains hand-crafted system files: systemd units
# (jackd.service, weston.service, openrig.service), environment.d, plymouth
# theme, and the eMMC installer script. The OpenRig binary/libs/data/assets
# are NOT in here — they come from the .deb in step 3.
echo ">>> [OpenRig] Copying rootfs overlay (systemd, plymouth, helpers)..."
cp -r /tmp/overlay/etc /
cp -r /tmp/overlay/usr /

# ── 3. Install OpenRig from the staged .deb ──────────────────────────────────
echo ">>> [OpenRig] Installing openrig.deb..."
DEBIAN_FRONTEND=noninteractive apt-get install -y --no-install-recommends \
    /tmp/overlay/openrig.deb

# The .deb stages shared libraries under /usr/lib/openrig/libs/ (per FHS),
# but OpenRig resolves relative asset paths against `<exe>/../share/openrig`
# (detect_data_root in infra-filesystem), which maps /usr/bin/openrig to
# /usr/share/openrig. Without this symlink, LV2 and NAM shared libs would
# not be found at runtime. This is a workaround for the current .deb
# layout — remove it once the packaging stages libs under
# /usr/share/openrig/libs/ directly.
if [ ! -e /usr/share/openrig/libs ]; then
    ln -sf /usr/lib/openrig/libs /usr/share/openrig/libs
fi

rm -rf /var/lib/apt/lists/*

# ── 5/6. Plymouth theme (DISABLED — debug mode) ──────────────────────────────
# Re-enable once boot sequence is stable:
#   - install plymouth + librsvg2-bin in step 1
#   - uncomment rsvg-convert + update-alternatives below
#   - restore 'splash' in KERNEL_ARGS (step 11)
#   - restore Plymouth commands in weston.service
# rsvg-convert -w 256 -h 256 /tmp/overlay/openrig-logomark.svg \
#     -o /usr/share/plymouth/themes/openrig/logo.png
# rsvg-convert -w 400 /tmp/overlay/openrig-logotype.svg \
#     -o /usr/share/plymouth/themes/openrig/logotype.png
# update-alternatives --install /usr/share/plymouth/themes/default.plymouth \
#     default.plymouth /usr/share/plymouth/themes/openrig/openrig.plymouth 100
# update-alternatives --set default.plymouth \
#     /usr/share/plymouth/themes/openrig/openrig.plymouth
echo ">>> [OpenRig] Plymouth theme disabled (debug mode)."

# ── 7. Create users with fixed passwords ─────────────────────────────────────
# openrig: regular user with a real home so OpenRig can write projects,
# presets and logs to /home/openrig. Groups cover everything audio/graphics
# related on Armbian: audio (jack rtprio/memlock), video (DRM), tty/input/
# render (logind seat access), plugdev (USB devices), dialout (serial).
# Both accounts get simple fixed passwords so we can SSH/console in for
# recovery — this is a dev/appliance image, not internet-facing.
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

# Create the default project config that openrig.service uses
# (ExecStart=/usr/bin/openrig --fullscreen --auto-save /etc/openrig.yaml).
# OpenRig opens directly into the main view instead of the launcher.
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
          - device_id: ""
            mode: mono
            channels: [0]
      - type: output
        model: standard
        enabled: true
        entries:
          - device_id: ""
            mode: stereo
            channels: [0, 1]
DEFAULT_PROJECT

# ── 8. Locale, keyboard, timezone ────────────────────────────────────────────
# English UI, Brazilian ABNT2 keyboard, São Paulo time. All configured
# directly so the Armbian first-run wizard has nothing left to ask.
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
# JACK needs real-time scheduling and memory locking for glitch-free audio.
# setcap grants these without running jackd as a privileged systemd service
# (JACK is now launched as a background process by OpenRig).
echo ">>> [OpenRig] Setting JACK RT capabilities and audio group membership..."
setcap cap_sys_nice,cap_ipc_lock=ep /usr/bin/jackd || true
usermod -aG audio root

# ── 9. Enable systemd services ───────────────────────────────────────────────
# JACK is managed programmatically by OpenRig (ensure_jack_running in
# infra-cpal), NOT by systemd. Mask jackd.service to prevent accidental
# activation. The jackd2 package is still installed for /usr/bin/jackd.
echo ">>> [OpenRig] Enabling services, masking jackd..."
systemctl disable jackd.service  2>/dev/null || true
systemctl mask    jackd.service  2>/dev/null || true
systemctl enable weston.service
systemctl enable openrig.service
systemctl enable openrig-irq-affinity.service
systemctl enable openrig-audio-watchdog.service


# ── 10. Set permissions on install script ────────────────────────────────────
chmod 755 /usr/local/bin/openrig-install-to-emmc
chmod 755 /usr/local/bin/openrig-reset-audio
chmod 755 /usr/local/bin/openrig-audio-watchdog

# ── 10a. USB-C TCPM workaround (RK3588 USB-C port stability) ─────────────────
# Root cause (issue #225): the FUSB302/TCPM stack on the RK3588 USB-C port
# sporadically misreads CC lines as 0V under sustained USB traffic and cuts
# VBUS via the vbus_typec regulator, which tears down xhci-hcd.7.auto and
# drops whatever device was plugged in. Affects ANY USB device on the USB-C
# port, not a single vendor — we reproduced it with a Focusrite Scarlett 2i2
# Gen 4 only because isoc audio is the workload OpenRig always runs.
# /sys/kernel/debug/usb/tcpm-6-0022/log shows the exact sequence: "CC1: 2
# -> 0, CC2: 1 -> 0 [disconnected]" → "VBUS off" on a working, actively
# streaming device.
#
# Fix: ship a DTB overlay (orange-pi/dtbo/openrig-usbc-host.dts) that disables
# the FUSB302 node, pins vbus_typec hard on, and forces DWC3 @fc000000 into
# pure host mode. Fully reversible by removing it from armbianEnv.txt.
#
# armbian-add-overlay compiles the .dts with dtc, installs the .dtbo into
# /boot/overlay-user/ and appends the overlay name to the user_overlays= line
# in /boot/armbianEnv.txt.
echo ">>> [OpenRig] Installing USB-C host-mode DTB overlay (Scarlett stability)..."
if command -v armbian-add-overlay >/dev/null 2>&1; then
    armbian-add-overlay /tmp/overlay/openrig-usbc-host.dts || \
        echo ">>> [OpenRig] WARNING: armbian-add-overlay failed, skipping USB-C overlay"
else
    # Fallback: compile with dtc and edit armbianEnv.txt by hand. This path is
    # only exercised if armbian-bsp-cli is ever dropped from the base image.
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

# ── 11. Boot configuration (debug mode) ──────────────────────────────────────
# Verbose boot: all kernel and systemd messages visible on HDMI (tty1).
# No Plymouth, no quiet, gettys kept active for emergency console access.
# Restore silent kiosk mode (quiet splash, console=tty3, mask gettys) once
# boot issues are identified and resolved.
echo ">>> [OpenRig] Configuring verbose debug boot..."

KERNEL_ARGS='loglevel=7 console=tty1 consoleblank=0'
if grep -q "^extraargs=" /boot/armbianEnv.txt 2>/dev/null; then
    sed -i "s|^extraargs=.*|extraargs=${KERNEL_ARGS}|" /boot/armbianEnv.txt
else
    echo "extraargs=${KERNEL_ARGS}" >> /boot/armbianEnv.txt
fi

# Armbian first-run wizard and filesystem resize — disable all of it.
# armbian-resize-filesystem.service hangs on noble despite the marker file;
# mask it explicitly so it never runs (SD card keeps image partition size).
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

echo ">>> [OpenRig] Image customization complete."
