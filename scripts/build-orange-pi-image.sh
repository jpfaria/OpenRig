#!/usr/bin/env bash
# Build an OpenRig-customized Orange Pi 5B image from official Armbian trixie.
#
# Downloads the latest OpenRig .deb (or uses --local-deb), downloads the
# official Armbian community image for Orangepi5b trixie + vendor kernel
# 6.1.115, then injects OpenRig via Docker linux/arm64 chroot.
#
# Fast: no kernel compilation. Kernel comes prebuilt by Armbian maintainers.
# Total runtime: ~5-10 min (mostly download + apt install inside chroot).
#
# Usage:
#   ./scripts/build-orange-pi-image.sh                     # latest GH release
#   ./scripts/build-orange-pi-image.sh --version v1.2.0    # specific release
#   ./scripts/build-orange-pi-image.sh --local-deb ...     # local .deb
#   ./scripts/build-orange-pi-image.sh --dry-run           # print steps only
#
# Prerequisites:
#   - Docker Desktop running (uses linux/arm64 via qemu)
#   - gh       (only if not using --local-deb)
#   - xz, curl (both standard on macOS)

set -euo pipefail

PROJECT_ROOT="$(cd "$(dirname "$0")/.." && pwd)"
OUTPUT_DIR="$PROJECT_ROOT/output/orange-pi"
GITHUB_REPO="jpfaria/OpenRig"

# Official Armbian community build — kernel vendor 6.1.115 (stable RK3588 LTS,
# equivalent to what BRANCH=current would produce), trixie = Debian 13 which
# ships Mesa 25.0+ with panthor_dri.so, enabling Mali-G610 hardware
# acceleration on RK3588 (issue #312). Bookworm's Mesa 22.3 lacks Panthor, so
# the UI fell back to llvmpipe software rendering at 2-5 FPS.
# glibc 2.41 in trixie is forward-compatible with the OpenRig .deb that is
# cross-compiled on bookworm Docker (glibc 2.36).
BOARD="orangepi5b"
RELEASE="trixie"
KERNEL_TAG="vendor_6.1.115"
ARMBIAN_RELEASE="26.2.0-trunk.792"
ARMBIAN_IMG_NAME="Armbian_community_${ARMBIAN_RELEASE}_Orangepi5b_${RELEASE}_${KERNEL_TAG}_minimal.img"
ARMBIAN_URL="https://github.com/armbian/community/releases/download/${ARMBIAN_RELEASE}/${ARMBIAN_IMG_NAME}.xz"
ARMBIAN_XZ="$OUTPUT_DIR/$ARMBIAN_IMG_NAME.xz"
ARMBIAN_IMG="$OUTPUT_DIR/$ARMBIAN_IMG_NAME"
OUTPUT_IMG="$OUTPUT_DIR/Armbian_openrig_${RELEASE}.img"

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
            grep '^#' "$0" | head -20 | sed 's/^# //'
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
    command -v docker >/dev/null || missing+=("docker (Docker Desktop)")
    command -v curl   >/dev/null || missing+=("curl")
    command -v xz     >/dev/null || missing+=("xz")
    if [ -z "$LOCAL_DEB" ]; then
        command -v gh >/dev/null || missing+=("gh (brew install gh)")
    fi
    if [ ${#missing[@]} -gt 0 ]; then
        echo "ERROR: Missing prerequisites:"
        printf '  - %s\n' "${missing[@]}"
        exit 1
    fi
    if ! docker info >/dev/null 2>&1; then
        echo "ERROR: Docker daemon not running. Start Docker Desktop."
        exit 1
    fi
}

# ── Step 1: Stage OpenRig .deb ────────────────────────────────────────────────
stage_deb() {
    if [ -n "$LOCAL_DEB" ]; then
        step "1/4  Using local OpenRig .deb"
        [ -f "$LOCAL_DEB" ] || { echo "ERROR: Local .deb not found: $LOCAL_DEB"; exit 1; }
        # Absolutize so docker -v gets a valid bind-mount path
        RELEASE_DEB="$(cd "$(dirname "$LOCAL_DEB")" && pwd)/$(basename "$LOCAL_DEB")"
        echo "  Package: $RELEASE_DEB"
        return
    fi

    step "1/4  Downloading OpenRig arm64 .deb ($VERSION)"
    local download_dir="$PROJECT_ROOT/output/orange-pi-release"
    run mkdir -p "$download_dir"
    run sh -c "rm -f '$download_dir'/openrig_*_arm64.deb"

    if [ "$VERSION" = "latest" ]; then
        run gh release download \
            --repo "$GITHUB_REPO" \
            --pattern "openrig_*_arm64.deb" \
            --dir "$download_dir" \
            --clobber
    else
        run gh release download "$VERSION" \
            --repo "$GITHUB_REPO" \
            --pattern "openrig_*_arm64.deb" \
            --dir "$download_dir" \
            --clobber
    fi

    RELEASE_DEB=$(ls "$download_dir"/openrig_*_arm64.deb 2>/dev/null | head -1)
    [ -n "$RELEASE_DEB" ] || { echo "ERROR: No .deb downloaded"; exit 1; }
    echo "  Staged: $RELEASE_DEB"
}

# ── Step 2: Obtain the official Armbian trixie image (cached) ─────────────────
download_armbian() {
    step "2/4  Obtaining official Armbian trixie image"
    run mkdir -p "$OUTPUT_DIR"

    if [ -f "$ARMBIAN_IMG" ]; then
        echo "  Using cached image: $(basename "$ARMBIAN_IMG")"
    else
        if [ ! -f "$ARMBIAN_XZ" ]; then
            echo "  Downloading (~288 MB compressed)..."
            echo "  $ARMBIAN_URL"
            run curl -L -f -o "$ARMBIAN_XZ" "$ARMBIAN_URL"
        else
            echo "  Using cached .xz: $(basename "$ARMBIAN_XZ")"
        fi
        echo "  Decompressing (keep .xz for cache)..."
        run xz -d -k -v "$ARMBIAN_XZ"
    fi
    $DRY_RUN || ls -lh "$ARMBIAN_IMG"
}

# ── Step 3: Copy base image to output and grow filesystem headroom ────────────
prepare_output_image() {
    step "3/4  Preparing output image (+2G headroom for apt install)"
    run rm -f "$OUTPUT_IMG"
    run cp "$ARMBIAN_IMG" "$OUTPUT_IMG"
    # Armbian minimal is ~1.6G with almost no slack. We need room for apt
    # install of ~17 packages + openrig.deb (~160 MB). +2G is conservative.
    run truncate -s +2G "$OUTPUT_IMG"
}

# ── Step 4: Customize image via Docker linux/arm64 chroot ─────────────────────
customize_image() {
    step "4/4  Customizing image via Docker linux/arm64 chroot"

    # The chroot script runs inside an arm64 container via qemu emulation so
    # the apt install, dpkg -i and initramfs rebuild all execute native ARM
    # binaries — exactly what will run on the Orange Pi.
    run docker run --rm --privileged --platform linux/arm64 \
        -v "$OUTPUT_DIR:/work" \
        -v "$PROJECT_ROOT/platform/orange-pi:/platform:ro" \
        -v "$PROJECT_ROOT/crates/adapter-gui/ui/assets:/ui-assets:ro" \
        -v "$(dirname "$RELEASE_DEB"):/debs:ro" \
        -v "$PROJECT_ROOT/presets:/presets:ro" \
        -e OPENRIG_DEB_NAME="$(basename "$RELEASE_DEB")" \
        -e OUTPUT_IMG_BASENAME="$(basename "$OUTPUT_IMG")" \
        -e RELEASE="$RELEASE" \
        debian:trixie bash -eu -c '
set -eu
IMG=/work/"$OUTPUT_IMG_BASENAME"

echo ">>> Installing host tools in orchestrator container..."
apt-get update -qq
apt-get install -y --no-install-recommends util-linux e2fsprogs gdisk kpartx dmsetup >/dev/null 2>&1

echo ">>> Fixing GPT backup header after truncate..."
# truncate +2G leaves the GPT backup header at the original (now middle) offset.
# sgdisk -e moves it to the real end of the device so the extra space
# becomes addressable.
sgdisk -e "$IMG"

echo ">>> Resizing partition 1 via sgdisk (delete + recreate, preserving identity)..."
# parted -s ignores the "Fix/Ignore" GPT-alignment prompt and silently
# skips the resize, leaving the partition unchanged. Use sgdisk to
# delete and recreate partition 1 covering the whole disk, preserving
# start sector, type code, partition UUID and label so /etc/fstab and
# armbianEnv.txt references (which use PARTUUID) keep working.
PART_INFO=$(sgdisk -i 1 "$IMG")
START=$(printf "%s" "$PART_INFO"   | awk "/First sector/ {print \$3}")
TYPE_CODE=$(printf "%s" "$PART_INFO" | awk "/Partition GUID code/ {print \$4}")
PART_UUID=$(printf "%s" "$PART_INFO" | awk "/Partition unique GUID/ {print \$4}")
PART_NAME=$(printf "%s" "$PART_INFO" | awk -F\" "/Partition name/ {print \$2}")

echo "  start=$START type=$TYPE_CODE uuid=$PART_UUID name=$PART_NAME"

sgdisk -d 1 "$IMG"
if [ -n "$PART_NAME" ]; then
    sgdisk -n "1:${START}:0" -t "1:${TYPE_CODE}" -u "1:${PART_UUID}" -c "1:${PART_NAME}" "$IMG"
else
    sgdisk -n "1:${START}:0" -t "1:${TYPE_CODE}" -u "1:${PART_UUID}" "$IMG"
fi
sgdisk -e "$IMG"

echo ">>> Mapping partitions via kpartx..."
# Docker Desktop on Mac runs inside a lightweight linuxkit VM whose loop
# driver does not re-read the GPT after sgdisk rewrites it — losetup
# --partscan and partprobe both keep the old table ("The kernel is still
# using the old partition table"). kpartx sidesteps this by reading the
# table directly from the file and registering device-mapper targets
# at /dev/mapper/loopNp1 that point into the loop device.
KPARTX_OUT=$(kpartx -av "$IMG")
echo "$KPARTX_OUT"
# Extract the mapper name (e.g. "loop3p1") from: "add map loop3p1 (253:0): 0 ..."
MAPPED_NAME=$(echo "$KPARTX_OUT" | awk "/add map/ {print \$3; exit}")
if [ -z "$MAPPED_NAME" ]; then
    echo "ERROR: kpartx produced no mapping. Output above."
    exit 1
fi
ROOT_PART="/dev/mapper/$MAPPED_NAME"
LOOP="/dev/$(echo "$MAPPED_NAME" | sed "s/p[0-9]*\$//")"

# Give device-mapper a moment to materialize the node
for i in 1 2 3 4 5; do
    [ -e "$ROOT_PART" ] && break
    sleep 1
done
if [ ! -e "$ROOT_PART" ]; then
    echo "ERROR: $ROOT_PART did not materialize."
    ls -la /dev/mapper/ || true
    exit 1
fi
echo "  Loop device:    $LOOP"
echo "  Root partition: $ROOT_PART"

echo ">>> Growing filesystem..."
e2fsck -f -y "$ROOT_PART" || true
resize2fs "$ROOT_PART"

echo ">>> Mounting..."
mkdir -p /mnt/img
mount "$ROOT_PART" /mnt/img

echo ">>> Staging overlay (matches customize-image.sh /tmp/overlay/ contract)..."
mkdir -p /mnt/img/tmp/overlay
cp -r /platform/rootfs/etc /mnt/img/tmp/overlay/
cp -r /platform/rootfs/usr /mnt/img/tmp/overlay/
cp /debs/"$OPENRIG_DEB_NAME" /mnt/img/tmp/overlay/openrig.deb
cp /ui-assets/openrig-logomark.svg /mnt/img/tmp/overlay/
cp /ui-assets/openrig-logotype.svg /mnt/img/tmp/overlay/
cp /platform/dtbo/openrig-usbc-host.dts /mnt/img/tmp/overlay/

if [ -d /presets ]; then
    mkdir -p /mnt/img/etc/presets
    cp -r /presets/. /mnt/img/etc/presets/
fi

echo ">>> Binding /dev /proc /sys for chroot + DNS for apt..."
mount --bind /dev  /mnt/img/dev
mount --bind /proc /mnt/img/proc
mount --bind /sys  /mnt/img/sys
# Armbian images ship /etc/resolv.conf as a dangling symlink pointing at
# /run/systemd/resolve/stub-resolv.conf (systemd-resolved) which doesnt
# exist in the offline chroot. Record the target so we can restore it
# before unmounting, then replace with a real file that makes apt work.
RESOLV_SYMLINK_TARGET=""
if [ -L /mnt/img/etc/resolv.conf ]; then
    RESOLV_SYMLINK_TARGET=$(readlink /mnt/img/etc/resolv.conf)
    rm /mnt/img/etc/resolv.conf
elif [ -f /mnt/img/etc/resolv.conf ]; then
    mv /mnt/img/etc/resolv.conf /mnt/img/etc/resolv.conf.bak
fi
cp /etc/resolv.conf /mnt/img/etc/resolv.conf

echo ">>> Executing customize-image.sh in chroot..."
cp /platform/customize-image.sh /mnt/img/tmp/customize-image.sh
chmod +x /mnt/img/tmp/customize-image.sh
chroot /mnt/img /tmp/customize-image.sh "$RELEASE"

echo ">>> Cleaning up inside image..."
rm -rf /mnt/img/tmp/overlay
rm -f /mnt/img/tmp/customize-image.sh
# Restore the original resolv.conf (symlink target) so the target system
# uses its own resolver at boot, not the orchestrator hosts cached entries.
rm -f /mnt/img/etc/resolv.conf
if [ -n "$RESOLV_SYMLINK_TARGET" ]; then
    ln -s "$RESOLV_SYMLINK_TARGET" /mnt/img/etc/resolv.conf
elif [ -f /mnt/img/etc/resolv.conf.bak ]; then
    mv /mnt/img/etc/resolv.conf.bak /mnt/img/etc/resolv.conf
else
    : > /mnt/img/etc/resolv.conf
fi

echo ">>> Unmounting..."
umount /mnt/img/sys || true
umount /mnt/img/proc || true
umount /mnt/img/dev || true
umount /mnt/img
# kpartx -d releases the /dev/mapper/loopNp1 and the backing loop device
kpartx -dv "$IMG" || true
losetup -d "$LOOP" 2>/dev/null || true
sync

echo ">>> Image customized successfully."
'
}

# ── Main ──────────────────────────────────────────────────────────────────────
echo "OpenRig — Orange Pi 5B Image Builder (official Armbian trixie base)"
echo "Repo:     github.com/$GITHUB_REPO"
echo "Board:    $BOARD"
echo "Release:  $RELEASE (Debian 13, Mesa 25+ with panthor_dri.so)"
echo "Kernel:   $KERNEL_TAG (Armbian community prebuilt)"
echo "Version:  $VERSION"
echo "DryRun:   $DRY_RUN"
[ -n "$LOCAL_DEB" ] && echo "LocalDeb: $LOCAL_DEB"
echo ""

check_prereqs
stage_deb
download_armbian
prepare_output_image
customize_image

echo ""
echo "══════════════════════════════════════════"
echo "  Done"
echo "══════════════════════════════════════════"
echo "Image ready: $OUTPUT_IMG"
echo ""
echo "Flash with:  ./scripts/flash-sd.sh $OUTPUT_IMG"
