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
#   - cross  (cargo install cross)
#   - Rust aarch64 target  (rustup target add aarch64-unknown-linux-gnu)
#   - librsvg  (brew install librsvg)  — for logo PNG conversion on macOS

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
            grep '^#' "$0" | head -15 | sed 's/^# //'
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
    command -v docker      >/dev/null || missing+=("docker")
    command -v cross       >/dev/null || missing+=("cross  (cargo install cross)")
    command -v rsvg-convert >/dev/null || missing+=("rsvg-convert  (brew install librsvg)")
    if ! rustup target list --installed 2>/dev/null | grep -q "$RUST_TARGET"; then
        missing+=("Rust target $RUST_TARGET  (rustup target add $RUST_TARGET)")
    fi
    if [ ${#missing[@]} -gt 0 ]; then
        echo "ERROR: Missing prerequisites:"
        printf '  - %s\n' "${missing[@]}"
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
    step "2/4  Cross-compile OpenRig binary (aarch64)"
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

    # Set up userpatches directory
    run mkdir -p "$USERPATCHES_DIR"
    run cp "$PROJECT_ROOT/orange-pi/customize-image.sh" "$USERPATCHES_DIR/customize-image.sh"
    run chmod +x "$USERPATCHES_DIR/customize-image.sh"

    # Stage binary
    run mkdir -p "$OVERLAY_DIR/bin"
    run cp "$PROJECT_ROOT/target/$RUST_TARGET/release/openrig" "$OVERLAY_DIR/bin/openrig"

    # Stage C++ libs
    run mkdir -p "$OVERLAY_DIR/lib/lv2"
    run mkdir -p "$OVERLAY_DIR/lib/nam"
    if [ -d "$PROJECT_ROOT/libs/lv2/linux-aarch64" ]; then
        run cp -r "$PROJECT_ROOT/libs/lv2/linux-aarch64/." "$OVERLAY_DIR/lib/lv2/"
    fi
    if [ -d "$PROJECT_ROOT/libs/nam/linux-aarch64" ]; then
        run cp -r "$PROJECT_ROOT/libs/nam/linux-aarch64/." "$OVERLAY_DIR/lib/nam/"
    fi

    # Stage logo SVG (rsvg-convert runs inside the Armbian chroot to produce logo.png)
    LOGO_SVG="$PROJECT_ROOT/crates/adapter-gui/ui/assets/openrig-logomark.svg"
    run cp "$LOGO_SVG" "$OVERLAY_DIR/openrig-logomark.svg"

    # Stage rootfs overlay (etc, usr)
    run cp -r "$PROJECT_ROOT/orange-pi/rootfs/." "$OVERLAY_DIR/"
}

# ── Step 4: Run Armbian build ─────────────────────────────────────────────────
run_armbian() {
    step "4/4  Running Armbian build (~30-60 min)"

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
    echo "Flash:  sudo dd if=\"\$OUTPUT_DIR/Armbian_*.img\" of=/dev/sdX bs=4M status=progress"
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
