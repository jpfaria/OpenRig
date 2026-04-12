#!/usr/bin/env bash
# Build a minimal Linux image for Orange Pi 5B running OpenRig.
#
# Downloads the latest OpenRig release for linux-aarch64 from GitHub,
# then generates a bootable Armbian image with OpenRig pre-installed.
#
# Usage:
#   ./scripts/build-orange-pi-image.sh                        # use latest release
#   ./scripts/build-orange-pi-image.sh --version v1.2.0      # use specific version
#   ./scripts/build-orange-pi-image.sh --dry-run              # print steps, don't execute
#
# Prerequisites:
#   - Docker   (for Armbian build)
#   - gh       (GitHub CLI, for downloading releases)

set -euo pipefail

PROJECT_ROOT="$(cd "$(dirname "$0")/.." && pwd)"
ARMBIAN_DIR="$PROJECT_ROOT/output/armbian-build"
ARMBIAN_REPO="https://github.com/armbian/build.git"
ARMBIAN_BRANCH="main"
BOARD="orangepi5b"
# BRANCH=edge → latest mainline kernel. Needed for up-to-date Focusrite
# `scarlett-gen2` driver and xHCI fixes that matter for USB audio on RK3588.
BRANCH="edge"
# RELEASE=noble → Ubuntu 24.04 LTS. Newer JACK2 + audio stack than Debian
# Bookworm, supported until 2029, still gets a minimal Armbian image.
RELEASE="noble"
OUTPUT_DIR="$PROJECT_ROOT/output/orange-pi"
USERPATCHES_DIR="$ARMBIAN_DIR/userpatches"
OVERLAY_DIR="$USERPATCHES_DIR/overlay"
GITHUB_REPO="jpfaria/OpenRig"

VERSION="latest"
LOCAL_DEB=""
DRY_RUN=false

# ── Parse args ────────────────────────────────────────────────────────────────
while [ $# -gt 0 ]; do
    case "$1" in
        --version)
            VERSION="$2"
            shift 2
            ;;
        --local-deb)
            LOCAL_DEB="$2"
            shift 2
            ;;
        --dry-run)
            DRY_RUN=true
            shift
            ;;
        --help|-h)
            grep '^#' "$0" | head -12 | sed 's/^# //'
            exit 0
            ;;
        *)
            echo "Unknown argument: $1"
            exit 1
            ;;
    esac
done

# ── Helpers ───────────────────────────────────────────────────────────────────
run() {
    echo "  $ $*"
    $DRY_RUN || "$@"
}

step() {
    echo ""
    echo "══════════════════════════════════════════"
    echo "  $*"
    echo "══════════════════════════════════════════"
}

check_prereqs() {
    local missing=()
    command -v docker >/dev/null || missing+=("docker")
    command -v gh     >/dev/null || missing+=("gh  (brew install gh)")
    if [ ${#missing[@]} -gt 0 ]; then
        echo "ERROR: Missing prerequisites:"
        printf '  - %s\n' "${missing[@]}"
        exit 1
    fi

    # On macOS, Armbian requires Bash 5 — use homebrew bash if available
    if [ "$(uname)" = "Darwin" ]; then
        BASH5=/opt/homebrew/bin/bash
        [ -x "$BASH5" ] || BASH5=$(brew --prefix 2>/dev/null)/bin/bash
        if [ ! -x "$BASH5" ]; then
            echo "ERROR: Bash 5 required on macOS. Run: brew install bash"
            exit 1
        fi
        export BASH="$BASH5"
    fi
}

# ── Step 1: Download or use local OpenRig .deb ──────────────────────────────
download_release() {
    if [ -n "$LOCAL_DEB" ]; then
        step "1/3  Using local .deb: $LOCAL_DEB"
        if [ ! -f "$LOCAL_DEB" ]; then
            echo "ERROR: Local .deb not found: $LOCAL_DEB"
            exit 1
        fi
        RELEASE_DEB="$LOCAL_DEB"
        echo "  Package: $RELEASE_DEB"
        return
    fi

    step "1/3  Downloading OpenRig arm64 .deb release ($VERSION)"

    local download_dir="$PROJECT_ROOT/output/orange-pi-release"
    run mkdir -p "$download_dir"
    # Clean any previous downloads so the 'ls | head' below always picks up the
    # package for the current version.
    run sh -c "rm -f '$download_dir'/openrig_*_arm64.deb"

    if [ "$VERSION" = "latest" ]; then
        echo "  Fetching latest release from github.com/$GITHUB_REPO..."
        run gh release download \
            --repo "$GITHUB_REPO" \
            --pattern "openrig_*_arm64.deb" \
            --dir "$download_dir" \
            --clobber
    else
        echo "  Fetching release $VERSION from github.com/$GITHUB_REPO..."
        run gh release download "$VERSION" \
            --repo "$GITHUB_REPO" \
            --pattern "openrig_*_arm64.deb" \
            --dir "$download_dir" \
            --clobber
    fi

    RELEASE_DEB=$(ls "$download_dir"/openrig_*_arm64.deb 2>/dev/null | head -1)
    if [ -z "$RELEASE_DEB" ]; then
        echo "ERROR: No openrig_*_arm64.deb found in $download_dir"
        exit 1
    fi
    echo "  Package staged at: $RELEASE_DEB"
}

# ── Step 2: Prepare Armbian userpatches overlay ───────────────────────────────
prepare_overlay() {
    step "2/3  Preparing Armbian userpatches overlay"

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
    run cp "$PROJECT_ROOT/platform/orange-pi/customize-image.sh" "$USERPATCHES_DIR/customize-image.sh"
    run chmod +x "$USERPATCHES_DIR/customize-image.sh"

    # Stage the .deb package into the overlay. customize-image.sh will
    # `apt install` it inside the chroot so all runtime dependencies are
    # resolved by dpkg — no manual file copies.
    run mkdir -p "$OVERLAY_DIR"
    run sh -c "rm -f '$OVERLAY_DIR'/openrig.deb"
    run cp "$RELEASE_DEB" "$OVERLAY_DIR/openrig.deb"

    # Stage logo SVG (rsvg-convert inside Armbian chroot converts to PNG)
    LOGO_SVG="$PROJECT_ROOT/crates/adapter-gui/ui/assets/openrig-logomark.svg"
    run cp "$LOGO_SVG" "$OVERLAY_DIR/openrig-logomark.svg"

    # Stage rootfs overlay (etc, usr)
    run cp -r "$PROJECT_ROOT/platform/orange-pi/rootfs/." "$OVERLAY_DIR/"

    # Stage DTB overlay source for the Scarlett/USB-C TCPM workaround.
    # customize-image.sh compiles and installs it inside the chroot using
    # armbian-add-overlay so it lands in /boot/overlay-user/ and gets hooked
    # into armbianEnv.txt automatically.
    run cp "$PROJECT_ROOT/platform/orange-pi/dtbo/openrig-usbc-host.dts" \
        "$OVERLAY_DIR/openrig-usbc-host.dts"

    # Stage PREEMPT_RT kernel config fragment
    run mkdir -p "$USERPATCHES_DIR/config/kernel"
    run cp "$PROJECT_ROOT/platform/orange-pi/kernel-config/orangepi5b-edge.config" \
        "$USERPATCHES_DIR/config/kernel/orangepi5b-edge.config"
}

# ── Step 3: Run Armbian build ─────────────────────────────────────────────────
run_armbian() {
    step "3/3  Running Armbian build (~30-60 min)"

    run mkdir -p "$OUTPUT_DIR"

    run "${BASH:-bash}" "$ARMBIAN_DIR/compile.sh" \
        "BOARD=$BOARD" \
        "BRANCH=$BRANCH" \
        "RELEASE=$RELEASE" \
        "BUILD_DESKTOP=no" \
        "BUILD_MINIMAL=yes" \
        "KERNEL_CONFIGURE=no" \
        "BOOT_LOGO=no" \
        "COMPRESS_OUTPUTIMAGE=no" \
        "OUTPUT_DIR=$OUTPUT_DIR"

    echo ""
    echo "Image written to: $OUTPUT_DIR/"
    echo "Flash with Balena Etcher or:"
    echo "  sudo dd if=\"$OUTPUT_DIR/Armbian_*.img\" of=/dev/sdX bs=4M status=progress"
}

# ── Main ──────────────────────────────────────────────────────────────────────
echo "OpenRig — Orange Pi 5B Image Builder"
echo "Repo:    github.com/$GITHUB_REPO"
echo "Version: $VERSION"
echo "Board:   $BOARD ($BRANCH, $RELEASE)"
echo "DryRun:  $DRY_RUN"
if [ -n "$LOCAL_DEB" ]; then
    echo "LocalDeb: $LOCAL_DEB"
fi
echo ""

check_prereqs
download_release
prepare_overlay
run_armbian

echo ""
echo "Done."
