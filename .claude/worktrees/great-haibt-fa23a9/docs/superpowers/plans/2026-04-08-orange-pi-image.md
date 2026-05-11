# Orange Pi 5B Image Build Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Script that cross-compiles OpenRig and generates a minimal flashable Linux image for Orange Pi 5B with boot splash, audio, and UI configured out of the box.

**Architecture:** Armbian Build Framework generates a minimal Debian Bookworm CLI image for `orangepi5b`. A `customize-image.sh` hook installs runtime deps, copies the OpenRig binary and libs, configures ALSA for the Teyun Q-26 USB interface, sets up a systemd service, and installs a Plymouth boot splash. The Rust binary is cross-compiled for `aarch64-unknown-linux-gnu` using the `cross` tool; C++ libs (NAM, LV2) via the existing Docker infra.

**Tech Stack:** Bash, Armbian Build Framework, `cross` (Rust cross-compilation), ALSA, Slint `linuxkms` + software renderer, Plymouth, systemd.

---

## File Map

| Action | Path | Purpose |
|--------|------|---------|
| Modify | `crates/adapter-gui/Cargo.toml` | Add Slint `backend-linuxkms` + `renderer-software` features |
| Modify | `.gitignore` | Ignore `.orange-pi-build/` and `output/` |
| Create | `orange-pi/customize-image.sh` | Armbian chroot hook — installs deps, copies files, enables service |
| Create | `orange-pi/rootfs/etc/asound.conf` | ALSA: pin Teyun Q-26 as default device |
| Create | `orange-pi/rootfs/etc/environment.d/50-slint.conf` | Slint linuxkms + software renderer env vars |
| Create | `orange-pi/rootfs/etc/systemd/system/openrig.service` | systemd unit: start OpenRig on boot |
| Create | `orange-pi/rootfs/usr/share/plymouth/themes/openrig/openrig.plymouth` | Plymouth theme descriptor |
| Create | `orange-pi/rootfs/usr/share/plymouth/themes/openrig/openrig.script` | Plymouth script: logo on black background |
| Create | `scripts/build-orange-pi-image.sh` | Entry point: cross-compile → Armbian build → .img output |

---

## Task 1: Setup solver workspace

**Files:** none (git + workspace setup)

- [ ] **Step 1: Verify issue branch doesn't already exist**

```bash
git fetch origin
git branch -a | grep "issue-225"
```

Expected: no output (branch doesn't exist yet).

- [ ] **Step 2: Create isolated solver workspace**

```bash
cd /Users/joao.faria/Projetos/github.com/jpfaria/OpenRig
rsync -a --exclude='target' --exclude='.solvers' --exclude='.orange-pi-build' --exclude='output' . .solvers/issue-225/
cd .solvers/issue-225
git checkout develop && git pull origin develop
git checkout -b feature/issue-225-orange-pi-image
```

Expected: new branch `feature/issue-225-orange-pi-image` from latest develop.

---

## Task 2: .gitignore + Slint feature gate

**Files:**
- Modify: `.gitignore`
- Modify: `crates/adapter-gui/Cargo.toml`

- [ ] **Step 1: Add .gitignore entries**

In `.solvers/issue-225/.gitignore`, append:

```
.orange-pi-build/
output/
```

- [ ] **Step 2: Add Slint linuxkms + software renderer to adapter-gui**

In `.solvers/issue-225/crates/adapter-gui/Cargo.toml`, change:

```toml
slint.workspace = true
```

to:

```toml
slint = { workspace = true, features = ["backend-linuxkms", "renderer-software"] }
```

This adds the linuxkms and software renderer on top of the workspace default (winit). Both backends compile in; the runtime env var `SLINT_BACKEND=linuxkms` selects linuxkms on Orange Pi.

- [ ] **Step 3: Verify desktop build still compiles**

```bash
cd .solvers/issue-225
cargo build -p adapter-gui 2>&1 | tail -5
```

Expected: `Finished` with no errors. Warnings about unused linuxkms backend are acceptable.

- [ ] **Step 4: Commit**

```bash
cd .solvers/issue-225
git add .gitignore crates/adapter-gui/Cargo.toml
git commit -m "feat(orange-pi): add linuxkms + software renderer to adapter-gui slint features"
```

---

## Task 3: ALSA and Slint config files

**Files:**
- Create: `orange-pi/rootfs/etc/asound.conf`
- Create: `orange-pi/rootfs/etc/environment.d/50-slint.conf`

- [ ] **Step 1: Create directory structure**

```bash
cd .solvers/issue-225
mkdir -p orange-pi/rootfs/etc/environment.d
mkdir -p orange-pi/rootfs/etc/systemd/system
mkdir -p orange-pi/rootfs/usr/share/plymouth/themes/openrig
```

- [ ] **Step 2: Write ALSA config**

Create `orange-pi/rootfs/etc/asound.conf`:

```
# Default audio device: Teyun Q-26 (USB)
# The udev rule in customize-image.sh pins it as card "Q26".
# Verify on first boot with: aplay -l

pcm.!default {
    type hw
    card Q26
}

ctl.!default {
    type hw
    card Q26
}
```

- [ ] **Step 3: Write Slint environment config**

Create `orange-pi/rootfs/etc/environment.d/50-slint.conf`:

```
SLINT_BACKEND=linuxkms
SLINT_RENDERER=software
```

- [ ] **Step 4: Verify files exist**

```bash
ls orange-pi/rootfs/etc/asound.conf orange-pi/rootfs/etc/environment.d/50-slint.conf
```

Expected: both files listed.

- [ ] **Step 5: Commit**

```bash
cd .solvers/issue-225
git add orange-pi/rootfs/etc/
git commit -m "feat(orange-pi): add ALSA and Slint environment config"
```

---

## Task 4: systemd service

**Files:**
- Create: `orange-pi/rootfs/etc/systemd/system/openrig.service`

- [ ] **Step 1: Write the service file**

Create `orange-pi/rootfs/etc/systemd/system/openrig.service`:

```ini
[Unit]
Description=OpenRig Guitar Pedalboard
Documentation=https://github.com/jpfaria/OpenRig
After=systemd-udev-settle.service sound.target local-fs.target
Wants=systemd-udev-settle.service sound.target

[Service]
Type=simple
User=openrig
Group=openrig
SupplementaryGroups=audio video

# Wait for the Teyun Q-26 to be enumerated (max 10s)
ExecStartPre=/bin/sh -c 'for i in $(seq 1 10); do aplay -l 2>/dev/null | grep -q Q26 && break; sleep 1; done'

Environment=SLINT_BACKEND=linuxkms
Environment=SLINT_RENDERER=software
Environment=RUST_LOG=warn

ExecStart=/usr/local/bin/openrig
Restart=on-failure
RestartSec=5
TimeoutStartSec=30

[Install]
WantedBy=multi-user.target
```

- [ ] **Step 2: Verify syntax**

```bash
systemd-analyze verify orange-pi/rootfs/etc/systemd/system/openrig.service 2>&1 || true
```

If `systemd-analyze` is not available (macOS), check manually that `After=`, `ExecStart=`, and `WantedBy=` are all present. The `|| true` prevents failure on macOS.

- [ ] **Step 3: Commit**

```bash
cd .solvers/issue-225
git add orange-pi/rootfs/etc/systemd/system/openrig.service
git commit -m "feat(orange-pi): add OpenRig systemd service"
```

---

## Task 5: Plymouth boot splash

**Files:**
- Create: `orange-pi/rootfs/usr/share/plymouth/themes/openrig/openrig.plymouth`
- Create: `orange-pi/rootfs/usr/share/plymouth/themes/openrig/openrig.script`

- [ ] **Step 1: Write Plymouth theme descriptor**

Create `orange-pi/rootfs/usr/share/plymouth/themes/openrig/openrig.plymouth`:

```ini
[Plymouth Theme]
Name=OpenRig
Description=OpenRig boot splash
ModuleName=script

[script]
ImageDir=/usr/share/plymouth/themes/openrig
ScriptFile=/usr/share/plymouth/themes/openrig/openrig.script
```

- [ ] **Step 2: Write Plymouth script**

Create `orange-pi/rootfs/usr/share/plymouth/themes/openrig/openrig.script`:

```javascript
// Black background
Window.SetBackgroundTopColor(0.0, 0.0, 0.0);
Window.SetBackgroundBottomColor(0.0, 0.0, 0.0);

// Centered logo
logo.image = Image("logo.png");
logo.sprite = Sprite(logo.image);
logo.sprite.SetX(Window.GetWidth()  / 2 - logo.image.GetWidth()  / 2);
logo.sprite.SetY(Window.GetHeight() / 2 - logo.image.GetHeight() / 2);
logo.sprite.SetOpacity(1);
```

The `logo.png` is generated by `customize-image.sh` from `openrig-logomark.svg` using `rsvg-convert`.

- [ ] **Step 3: Commit**

```bash
cd .solvers/issue-225
git add orange-pi/rootfs/usr/share/plymouth/
git commit -m "feat(orange-pi): add Plymouth OpenRig boot splash theme"
```

---

## Task 6: customize-image.sh (Armbian chroot hook)

**Files:**
- Create: `orange-pi/customize-image.sh`

This script runs **inside the Armbian chroot** (i.e., inside the target image). The host's `orange-pi/rootfs/` overlay is accessible at `/tmp/overlay/` inside the chroot.

- [ ] **Step 1: Write the hook**

Create `orange-pi/customize-image.sh`:

```bash
#!/bin/bash
# Armbian customize-image.sh hook.
# Runs INSIDE the image chroot after base packages are installed.
# /tmp/overlay/ contains the contents of orange-pi/rootfs/.
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

# ── 2. Copy rootfs overlay ────────────────────────────────────────────────────
echo ">>> [OpenRig] Copying rootfs overlay..."
cp -r /tmp/overlay/etc /
cp -r /tmp/overlay/usr /

# ── 3. Convert OpenRig logo for Plymouth ─────────────────────────────────────
echo ">>> [OpenRig] Converting logo to PNG..."
rsvg-convert \
    -w 256 -h 256 \
    /tmp/overlay/openrig-logomark.svg \
    -o /usr/share/plymouth/themes/openrig/logo.png

# ── 4. Register and activate Plymouth theme ──────────────────────────────────
echo ">>> [OpenRig] Activating Plymouth theme..."
update-alternatives --install \
    /usr/share/plymouth/themes/default.plymouth \
    default.plymouth \
    /usr/share/plymouth/themes/openrig/openrig.plymouth \
    100
update-alternatives --set \
    default.plymouth \
    /usr/share/plymouth/themes/openrig/openrig.plymouth

# ── 5. Create openrig system user ────────────────────────────────────────────
echo ">>> [OpenRig] Creating openrig user..."
useradd --system --no-create-home \
    --groups audio,video \
    --shell /usr/sbin/nologin \
    openrig

# ── 6. Install OpenRig binary and libs ───────────────────────────────────────
echo ">>> [OpenRig] Installing OpenRig binary..."
install -m 755 /tmp/overlay/bin/openrig /usr/local/bin/openrig

echo ">>> [OpenRig] Installing LV2 libs..."
mkdir -p /usr/local/lib/openrig
cp -r /tmp/overlay/lib/lv2 /usr/local/lib/openrig/
cp -r /tmp/overlay/lib/nam /usr/local/lib/openrig/

# ── 7. Add udev rule to pin Teyun Q-26 as card "Q26" ─────────────────────────
# USB ID for Teyun Q-26: 0x1852:0x5065 (verify with `lsusb` on first boot if card isn't found)
cat > /etc/udev/rules.d/90-teyun-q26.rules <<'EOF'
# Teyun Q-26 USB Audio Interface
SUBSYSTEM=="sound", ATTR{id}=="Q26", GOTO="q26_done"
SUBSYSTEM=="sound", SUBSYSTEMS=="usb", \
  ATTRS{idVendor}=="1852", ATTRS{idProduct}=="5065", \
  ATTR{id}="Q26"
LABEL="q26_done"
EOF

# ── 8. Enable systemd service ─────────────────────────────────────────────────
echo ">>> [OpenRig] Enabling openrig.service..."
systemctl enable openrig.service

echo ">>> [OpenRig] Image customization complete."
```

- [ ] **Step 2: Make executable**

```bash
chmod +x .solvers/issue-225/orange-pi/customize-image.sh
```

- [ ] **Step 3: Syntax check**

```bash
bash -n .solvers/issue-225/orange-pi/customize-image.sh && echo "OK: syntax valid"
```

Expected: `OK: syntax valid`

- [ ] **Step 4: ShellCheck (if available)**

```bash
shellcheck .solvers/issue-225/orange-pi/customize-image.sh || true
```

Expected: no errors (warnings about `$1` usage are acceptable).

- [ ] **Step 5: Commit**

```bash
cd .solvers/issue-225
git add orange-pi/customize-image.sh
git commit -m "feat(orange-pi): add Armbian chroot customize-image hook"
```

---

## Task 7: build-orange-pi-image.sh (entry point)

**Files:**
- Create: `scripts/build-orange-pi-image.sh`

- [ ] **Step 1: Write the script**

Create `scripts/build-orange-pi-image.sh`:

```bash
#!/usr/bin/env bash
# Build a minimal Linux image for Orange Pi 5B running OpenRig.
#
# Usage:
#   ./scripts/build-orange-pi-image.sh             # full build
#   ./scripts/build-orange-pi-image.sh --dry-run   # print steps, don't execute
#   ./scripts/build-orange-pi-image.sh --skip-libs # skip C++ lib cross-compilation
#   ./scripts/build-orange-pi-image.sh --skip-rust # skip Rust cross-compilation
#
# Prerequisites:
#   - Docker (for Armbian build and C++ libs)
#   - cross (cargo install cross)
#   - Rust aarch64 target (rustup target add aarch64-unknown-linux-gnu)
#   - librsvg (brew install librsvg) — for logo PNG conversion

set -euo pipefail

PROJECT_ROOT="$(cd "$(dirname "$0")/.." && pwd)"
ARMBIAN_DIR="$PROJECT_ROOT/.orange-pi-build"
ARMBIAN_REPO="https://github.com/armbian/build.git"
ARMBIAN_BRANCH="main"
BOARD="orangepi5b"
BRANCH="current"
RELEASE="bookworm"
OUTPUT_DIR="$PROJECT_ROOT/output/orange-pi"
USERPATCHES_DIR="$ARMBIAN_DIR/userpatches"
OVERLAY_DIR="$USERPATCHES_DIR/overlay"
RUST_TARGET="aarch64-unknown-linux-gnu"

DRY_RUN=false
SKIP_LIBS=false
SKIP_RUST=false

# ── Parse args ────────────────────────────────────────────────────────────────
for arg in "$@"; do
    case "$arg" in
        --dry-run)   DRY_RUN=true ;;
        --skip-libs) SKIP_LIBS=true ;;
        --skip-rust) SKIP_RUST=true ;;
        --help|-h)
            sed -n '/^# Usage/,/^[^#]/p' "$0" | head -n -1 | sed 's/^# //'
            exit 0
            ;;
        *)
            echo "Unknown argument: $arg"
            exit 1
            ;;
    esac
done

# ── Helpers ───────────────────────────────────────────────────────────────────
run() {
    echo "  $ $*"
    if ! $DRY_RUN; then
        "$@"
    fi
}

step() {
    echo ""
    echo "══════════════════════════════════════════"
    echo "  $1"
    echo "══════════════════════════════════════════"
}

check_prereqs() {
    local missing=()
    command -v docker  >/dev/null || missing+=("docker")
    command -v cross   >/dev/null || missing+=("cross  (cargo install cross)")
    command -v rsvg-convert >/dev/null || missing+=("rsvg-convert  (brew install librsvg)")
    if ! rustup target list --installed | grep -q "$RUST_TARGET"; then
        missing+=("Rust target $RUST_TARGET  (rustup target add $RUST_TARGET)")
    fi
    if [ ${#missing[@]} -gt 0 ]; then
        echo "ERROR: Missing prerequisites:"
        for m in "${missing[@]}"; do echo "  - $m"; done
        exit 1
    fi
}

# ── Step 1: Cross-compile C++ libs ───────────────────────────────────────────
build_cpp_libs() {
    step "1/4  Cross-compile C++ libs for linux-aarch64"
    run bash "$PROJECT_ROOT/scripts/build-lib.sh" all --platform linux-aarch64
}

# ── Step 2: Cross-compile OpenRig binary ─────────────────────────────────────
build_rust() {
    step "2/4  Cross-compile OpenRig binary"
    run cross build \
        --target "$RUST_TARGET" \
        --release \
        --manifest-path "$PROJECT_ROOT/Cargo.toml" \
        -p adapter-gui
    echo "  Binary: target/$RUST_TARGET/release/openrig"
}

# ── Step 3: Prepare Armbian userpatches overlay ───────────────────────────────
prepare_overlay() {
    step "3/4  Preparing Armbian userpatches overlay"

    # Clone or update Armbian build repo
    if [ ! -d "$ARMBIAN_DIR/.git" ]; then
        echo "  Cloning Armbian build framework..."
        run git clone --depth=1 --branch "$ARMBIAN_BRANCH" "$ARMBIAN_REPO" "$ARMBIAN_DIR"
    else
        echo "  Updating Armbian build framework..."
        run git -C "$ARMBIAN_DIR" pull --ff-only
    fi

    # Set up userpatches
    run mkdir -p "$USERPATCHES_DIR"
    run cp "$PROJECT_ROOT/orange-pi/customize-image.sh" "$USERPATCHES_DIR/customize-image.sh"

    # Set up overlay (rootfs files + binary + libs + logo)
    run mkdir -p "$OVERLAY_DIR/bin"
    run mkdir -p "$OVERLAY_DIR/lib/lv2"
    run mkdir -p "$OVERLAY_DIR/lib/nam"
    run cp "$PROJECT_ROOT/target/$RUST_TARGET/release/openrig" "$OVERLAY_DIR/bin/openrig"
    run cp -r "$PROJECT_ROOT/libs/lv2/linux-aarch64/." "$OVERLAY_DIR/lib/lv2/"
    run cp -r "$PROJECT_ROOT/libs/nam/linux-aarch64/." "$OVERLAY_DIR/lib/nam/"

    # Copy logo SVG to overlay — rsvg-convert runs inside the Armbian chroot to produce logo.png
    LOGO_SVG="$PROJECT_ROOT/crates/adapter-gui/ui/assets/openrig-logomark.svg"
    run cp "$LOGO_SVG" "$OVERLAY_DIR/openrig-logomark.svg"

    # Copy rootfs overlay
    run cp -r "$PROJECT_ROOT/orange-pi/rootfs/." "$OVERLAY_DIR/"
}

# ── Step 4: Run Armbian build ──────────────────────────────────────────────────
run_armbian() {
    step "4/4  Running Armbian build (this takes ~30-60 min)"

    run mkdir -p "$OUTPUT_DIR"

    run bash "$ARMBIAN_DIR/compile.sh" \
        BOARD="$BOARD" \
        BRANCH="$BRANCH" \
        RELEASE="$RELEASE" \
        BUILD_DESKTOP=no \
        BUILD_MINIMAL=yes \
        KERNEL_CONFIGURE=no \
        BOOT_LOGO=no \
        COMPRESS_OUTPUTIMAGE=no \
        OUTPUT_DIR="$OUTPUT_DIR"

    echo ""
    echo "Image written to: $OUTPUT_DIR/"
    echo "Flash with:  dd if=$OUTPUT_DIR/Armbian_*.img of=/dev/sdX bs=4M status=progress"
}

# ── Main ──────────────────────────────────────────────────────────────────────
echo "OpenRig — Orange Pi 5B Image Builder"
echo "Board:   $BOARD ($BRANCH, $RELEASE)"
echo "Target:  $RUST_TARGET"
echo "DryRun:  $DRY_RUN"
echo ""

check_prereqs

$SKIP_LIBS || build_cpp_libs
$SKIP_RUST || build_rust
prepare_overlay
run_armbian

echo ""
echo "Done."
```

- [ ] **Step 2: Make executable**

```bash
chmod +x .solvers/issue-225/scripts/build-orange-pi-image.sh
```

- [ ] **Step 3: Syntax check**

```bash
bash -n .solvers/issue-225/scripts/build-orange-pi-image.sh && echo "OK: syntax valid"
```

Expected: `OK: syntax valid`

- [ ] **Step 4: ShellCheck**

```bash
shellcheck .solvers/issue-225/scripts/build-orange-pi-image.sh || true
```

Expected: no errors.

- [ ] **Step 5: Dry-run smoke test**

```bash
cd .solvers/issue-225
bash scripts/build-orange-pi-image.sh --dry-run 2>&1 | head -40
```

Expected: prints all 4 steps with `$ ...` commands but exits cleanly without executing anything. Prerequisite check will fail if `cross`/`docker` aren't installed — that's fine, the dry-run path still shows the plan.

- [ ] **Step 6: Commit**

```bash
cd .solvers/issue-225
git add scripts/build-orange-pi-image.sh
git commit -m "feat(orange-pi): add build-orange-pi-image.sh entry point script"
```

---

## Task 8: Open PR

- [ ] **Step 1: Push branch**

```bash
cd .solvers/issue-225
git push -u origin feature/issue-225-orange-pi-image
```

- [ ] **Step 2: Create PR**

```bash
gh pr create \
  --base develop \
  --title "feat: script to generate minimal Linux image for Orange Pi 5B" \
  --body "$(cat <<'EOF'
## Summary

- `scripts/build-orange-pi-image.sh` — entry point: cross-compiles libs + Rust binary, then triggers Armbian build
- `orange-pi/customize-image.sh` — Armbian chroot hook: installs deps, copies binary/libs, configures ALSA, Plymouth, systemd
- `orange-pi/rootfs/` — ready-to-copy config: ALSA (Teyun Q-26), Slint env, systemd service, Plymouth theme
- `crates/adapter-gui/Cargo.toml` — adds Slint `backend-linuxkms` + `renderer-software` features

## How to build

```bash
./scripts/build-orange-pi-image.sh
# or dry-run:
./scripts/build-orange-pi-image.sh --dry-run
```

Closes #225
EOF
)"
```

- [ ] **Step 3: Checkout develop to test**

```bash
git checkout develop && git pull
```

---

## Notes for first boot

1. Plug in the Teyun Q-26 **before** powering on
2. If audio doesn't initialize, run `aplay -l` to check card name and verify the udev rule pinned it as `Q26`
3. ALSA card name can be confirmed with: `cat /proc/asound/cards`
4. If the USB Vendor:Product ID of the Q-26 differs from `1852:5065`, update `/etc/udev/rules.d/90-teyun-q26.rules` and run `udevadm control --reload-rules && udevadm trigger`
