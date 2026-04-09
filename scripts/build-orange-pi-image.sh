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
ARMBIAN_DIR="$PROJECT_ROOT/.orange-pi-build"
ARMBIAN_REPO="https://github.com/armbian/build.git"
ARMBIAN_BRANCH="main"
BOARD="orangepi5b"
BRANCH="current"
RELEASE="bookworm"
OUTPUT_DIR="$PROJECT_ROOT/output/orange-pi"
USERPATCHES_DIR="$ARMBIAN_DIR/userpatches"
OVERLAY_DIR="$USERPATCHES_DIR/overlay"
GITHUB_REPO="jpfaria/OpenRig"

VERSION="latest"
DRY_RUN=false

# ── Parse args ────────────────────────────────────────────────────────────────
while [ $# -gt 0 ]; do
    case "$1" in
        --version)
            VERSION="$2"
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

# ── Step 1: Download OpenRig release ─────────────────────────────────────────
download_release() {
    step "1/3  Downloading OpenRig linux-aarch64 release ($VERSION)"

    local download_dir="$PROJECT_ROOT/output/orange-pi-release"
    run mkdir -p "$download_dir"

    if [ "$VERSION" = "latest" ]; then
        echo "  Fetching latest release from github.com/$GITHUB_REPO..."
        run gh release download \
            --repo "$GITHUB_REPO" \
            --pattern "openrig-*-linux-aarch64.tar.gz" \
            --dir "$download_dir" \
            --clobber
    else
        echo "  Fetching release $VERSION from github.com/$GITHUB_REPO..."
        run gh release download "$VERSION" \
            --repo "$GITHUB_REPO" \
            --pattern "openrig-*-linux-aarch64.tar.gz" \
            --dir "$download_dir" \
            --clobber
    fi

    # Extract
    local tarball
    tarball=$(ls "$download_dir"/openrig-*-linux-aarch64.tar.gz 2>/dev/null | head -1)
    if [ -z "$tarball" ]; then
        echo "ERROR: No openrig-*-linux-aarch64.tar.gz found in $download_dir"
        exit 1
    fi

    echo "  Extracting: $(basename "$tarball")"
    run tar -xzf "$tarball" -C "$download_dir"

    # Find extracted directory
    RELEASE_DIR=$(ls -d "$download_dir"/openrig-*-linux-aarch64 2>/dev/null | head -1)
    if [ -z "$RELEASE_DIR" ]; then
        echo "ERROR: Could not find extracted release directory in $download_dir"
        exit 1
    fi
    echo "  Release staged at: $RELEASE_DIR"
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
    run cp "$PROJECT_ROOT/orange-pi/customize-image.sh" "$USERPATCHES_DIR/customize-image.sh"
    run chmod +x "$USERPATCHES_DIR/customize-image.sh"

    # Stage release contents into overlay
    run mkdir -p "$OVERLAY_DIR/openrig-release"
    run cp -r "$RELEASE_DIR/." "$OVERLAY_DIR/openrig-release/"

    # Stage logo SVG (rsvg-convert inside Armbian chroot converts to PNG)
    LOGO_SVG="$PROJECT_ROOT/crates/adapter-gui/ui/assets/openrig-logomark.svg"
    run cp "$LOGO_SVG" "$OVERLAY_DIR/openrig-logomark.svg"

    # Stage rootfs overlay (etc, usr)
    run cp -r "$PROJECT_ROOT/orange-pi/rootfs/." "$OVERLAY_DIR/"
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
echo ""

check_prereqs
download_release
prepare_overlay
run_armbian

echo ""
echo "Done."
