#!/bin/bash
# Armbian customize-image.sh hook.
# Runs INSIDE the image chroot after base packages are installed.
# /tmp/overlay/ contains the contents of orange-pi/rootfs/ plus the staged binary and libs.
set -euo pipefail

RELEASE="$1"   # e.g. "bookworm"

echo ">>> [OpenRig] Customizing image for release: $RELEASE"

# ── 1. Install runtime dependencies ──────────────────────────────────────────
# System-level packages that are NOT pulled in by the openrig .deb itself:
# weston/plymouth (boot + display stack), jackd2 (audio server), librsvg2-bin
# (used below to convert the logo to PNG for the boot splash). The openrig
# .deb declares its own runtime deps (libasound2 etc.), which apt will
# resolve when we install it in step 3.
echo "jackd2 jackd/tweak_rt_limits boolean true" | debconf-set-selections
DEBIAN_FRONTEND=noninteractive apt-get update -qq
DEBIAN_FRONTEND=noninteractive apt-get install -y --no-install-recommends \
    alsa-utils \
    jackd2 \
    libfreetype6 \
    libfontconfig1 \
    libdrm2 \
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

# ── 9. Enable systemd services ───────────────────────────────────────────────
echo ">>> [OpenRig] Enabling jackd.service, weston.service and openrig.service..."
systemctl enable jackd.service
systemctl enable weston.service
systemctl enable openrig.service

# ── 10. Set permissions on install script ────────────────────────────────────
chmod 755 /usr/local/bin/openrig-install-to-emmc

# ── 11. Silent kiosk boot ─────────────────────────────────────────────────────
# Pure appliance boot: plymouth splash from u-boot handoff all the way until
# weston takes the framebuffer, no text flashes, no login prompts on any tty,
# no Armbian first-run wizard, no cursor blink.
echo ">>> [OpenRig] Configuring silent kiosk boot..."

# Kernel cmdline:
#   quiet, loglevel=3            → suppress kernel chatter on the main console
#   splash                        → Plymouth takes over early
#   console=tty3                  → kernel console goes to an invisible tty
#   vt.global_cursor_default=0    → no blinking cursor on any vt
#   consoleblank=0                → never blank the console automatically
#   systemd.show_status=false     → hide systemd unit status lines
#   rd.systemd.show_status=false  → same, inside initramfs
#   rd.udev.log_level=3           → quiet udev in initramfs
KERNEL_ARGS='quiet splash loglevel=3 console=tty3 vt.global_cursor_default=0 consoleblank=0 systemd.show_status=false rd.systemd.show_status=false rd.udev.log_level=3'
if grep -q "^extraargs=" /boot/armbianEnv.txt 2>/dev/null; then
    sed -i "s|^extraargs=.*|extraargs=${KERNEL_ARGS}|" /boot/armbianEnv.txt
else
    echo "extraargs=${KERNEL_ARGS}" >> /boot/armbianEnv.txt
fi

# Armbian first-run wizard (language/keyboard/timezone/user prompt).
# On Armbian, the presence of /root/.not_logged_in_yet triggers the wizard
# on first shell login; armbian-firstrun*.service also runs unconditionally.
# We kill all of it.
rm -f /root/.not_logged_in_yet
touch /root/.no_rootfs_resize_at_firstboot
systemctl disable armbian-firstrun.service           2>/dev/null || true
systemctl disable armbian-firstrun-config.service    2>/dev/null || true
systemctl mask    armbian-firstrun.service           2>/dev/null || true
systemctl mask    armbian-firstrun-config.service    2>/dev/null || true
rm -f /etc/profile.d/armbian-check-first-run.sh
rm -f /etc/update-motd.d/30-armbian-sysinfo          2>/dev/null || true

# Mask every getty so no text login prompt can ever appear.
for tty in 1 2 3 4 5 6; do
    systemctl disable "getty@tty${tty}.service" 2>/dev/null || true
    systemctl mask    "getty@tty${tty}.service" 2>/dev/null || true
done
systemctl disable serial-getty@ttyS0.service  2>/dev/null || true
systemctl mask    serial-getty@ttyS0.service  2>/dev/null || true

echo ">>> [OpenRig] Image customization complete."
