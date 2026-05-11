# Orange Pi 5B Image — Design Spec

**Date:** 2026-04-08  
**Board:** Orange Pi 5B (Rockchip RK3588S, ARM64)  
**Goal:** Script that generates a minimal flashable Linux image with OpenRig running on boot.

---

## Hardware Target

| Item | Detail |
|------|--------|
| Board | Orange Pi 5B |
| SoC | Rockchip RK3588S (4× Cortex-A76 + 4× Cortex-A55) |
| Architecture | aarch64 |
| Display | HDMI (Slint linuxkms, software renderer) |
| Audio | Teyun Q-26 (USB class-compliant, ALSA) |

---

## Approach: Armbian Build Framework

Use the [Armbian build framework](https://github.com/armbian/build) to generate a minimal Debian Bookworm CLI image for the Orange Pi 5B. The Armbian `customize-image.sh` hook injects the OpenRig binary, assets, and system configuration into the chroot before the image is finalized.

---

## Build Flow

```
scripts/build-orange-pi-image.sh
  │
  ├── 1. Cross-compile C++ libs (NAM, LV2)
  │      ./scripts/build-lib.sh all --platform linux-aarch64
  │      → libs/lv2/linux-aarch64/
  │      → libs/nam/linux-aarch64/
  │
  ├── 2. Cross-compile OpenRig binary
  │      cross build --target aarch64-unknown-linux-gnu --release
  │      → target/aarch64-unknown-linux-gnu/release/openrig
  │
  └── 3. Armbian build (Docker)
         BOARD=orangepi5b BRANCH=current RELEASE=bookworm
         BUILD_DESKTOP=no BUILD_MINIMAL=yes
         hook: orange-pi/customize-image.sh
         → output/images/Armbian_OrangePi5B_*.img
```

---

## New Files

```
scripts/
  build-orange-pi-image.sh          — entry point (steps 1–3)

orange-pi/
  customize-image.sh                — Armbian chroot hook
  rootfs/
    etc/systemd/system/openrig.service
    etc/asound.conf
    etc/environment.d/50-slint.conf
    usr/share/plymouth/themes/openrig/
      openrig.plymouth
      openrig.script
      logo.png                      — converted from openrig-logomark.svg
```

---

## Slint Configuration

Slint runs via the `linuxkms` backend with the software renderer — no Wayland, no X11, no GPU driver required.

```
SLINT_BACKEND=linuxkms
SLINT_RENDERER=software
```

Set in `/etc/environment.d/50-slint.conf` so the systemd service inherits them.

The `openrig` user is added to the `video` group for DRM/KMS access.

---

## ALSA Configuration

The Teyun Q-26 is USB class-compliant (no custom driver needed). `/etc/asound.conf` sets it as the default PCM/CTL device:

```
defaults.pcm.card 1
defaults.ctl.card 1
```

Card index 1 is typical when the USB device is the only audio interface besides the onboard dummy. The actual device enumeration is validated at first boot; a udev rule pins the Q-26 by USB ID to ensure stable ordering.

---

## Systemd Service

`/etc/systemd/system/openrig.service`:

- Runs as dedicated user `openrig` (groups: `audio`, `video`)
- `WantedBy=multi-user.target` — starts on boot
- `Restart=on-failure`
- Environment: inherits `/etc/environment.d/50-slint.conf`
- `ExecStartPre` waits for the USB audio device to appear (udev settle)

---

## Boot Splash (Plymouth)

- Plymouth installed with `openrig` theme
- Theme: black background, `logo.png` (converted from `openrig-logomark.svg`) centered
- Kernel cmdline: `quiet splash` added via Armbian `BOOT_CMDLINE` config
- Plymouth hands off to the OpenRig systemd service without a login prompt (auto-login on TTY1 is disabled; the service starts directly)

Logo conversion: `rsvg-convert -w 256 -h 256 openrig-logomark.svg > logo.png` — runs during `customize-image.sh`.

---

## Image Size Estimate

| Component | Size |
|-----------|------|
| Armbian minimal bookworm | ~250 MB |
| Runtime deps (alsa, mesa-drm, fontconfig, plymouth) | ~60 MB |
| OpenRig binary | ~20 MB |
| LV2 + NAM libs | ~40 MB |
| Assets (captures, IRs, SVGs) | ~variable |
| **Total** | **~400–500 MB** |

---

## Constraints & Decisions

- **No Wayland/X11** — Slint linuxkms + software renderer is sufficient for a guitar pedalboard UI and eliminates compositor complexity.
- **Software renderer** — avoids Mali/Panfrost driver instability on RK3588S. Can be upgraded to GPU rendering later with a single env var change.
- **`cross` tool** — used for Rust cross-compilation. Handles sysroot and linker automatically via Docker.
- **Armbian `current` branch** — tracks the latest stable kernel with RK3588S patches. `edge` is too unstable for a product image.
- **No GUI login manager** — OpenRig starts as a systemd service directly. No desktop environment installed.
- **Teyun Q-26 pinned via udev** — USB audio card index can shift if other USB devices are connected; a udev rule pins it by vendor/product ID.
