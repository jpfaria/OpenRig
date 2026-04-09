#!/usr/bin/env bash
# Flash the OpenRig Orange Pi image to an SD card.
#
# Usage:
#   ./scripts/flash-sd.sh               # auto-detect latest .img
#   ./scripts/flash-sd.sh path/to/image.img
#
# Prerequisites:
#   - macOS
#   - The .img file in output/orange-pi/ (run build-orange-pi-image.sh first)

set -euo pipefail

PROJECT_ROOT="$(cd "$(dirname "$0")/.." && pwd)"
ARMBIAN_OUTPUT_DIR="$PROJECT_ROOT/.orange-pi-build/output/images"

# ── Find image ────────────────────────────────────────────────────────────────
if [ $# -ge 1 ]; then
    IMAGE="$1"
else
    IMAGE=$(ls -t "$ARMBIAN_OUTPUT_DIR"/Armbian*.img 2>/dev/null | head -1)
    if [ -z "$IMAGE" ]; then
        echo "ERROR: No .img found in $ARMBIAN_OUTPUT_DIR"
        echo "       Run ./scripts/build-orange-pi-image.sh first."
        exit 1
    fi
fi

if [ ! -f "$IMAGE" ]; then
    echo "ERROR: Image not found: $IMAGE"
    exit 1
fi

IMAGE_SIZE=$(du -sh "$IMAGE" | cut -f1)
echo "Image: $(basename "$IMAGE") ($IMAGE_SIZE)"
echo ""

# ── List external disks ───────────────────────────────────────────────────────
echo "Available external disks:"
echo ""
diskutil list external physical 2>/dev/null || diskutil list | grep -A4 "external"
echo ""

# ── Select disk ──────────────────────────────────────────────────────────────
read -rp "Enter disk identifier (e.g. disk2): " DISK

DISK="${DISK#/dev/}"   # strip /dev/ if user typed it
DISK_DEV="/dev/$DISK"
RAW_DEV="/dev/r$DISK"  # raw device (/dev/rdisk2) — faster on macOS

if ! diskutil info "$DISK_DEV" >/dev/null 2>&1; then
    echo "ERROR: Disk $DISK_DEV not found."
    exit 1
fi

DISK_INFO=$(diskutil info "$DISK_DEV" | grep -E "Device / Media Name|Total Size" | sed 's/^[[:space:]]*//')
echo ""
echo "Selected: $DISK_DEV"
echo "$DISK_INFO"
echo ""
echo "WARNING: ALL DATA ON $DISK_DEV WILL BE ERASED."
read -rp "Type YES to confirm: " CONFIRM

if [ "$CONFIRM" != "YES" ]; then
    echo "Aborted."
    exit 0
fi

# ── Flash ─────────────────────────────────────────────────────────────────────
echo ""
echo "Unmounting $DISK_DEV..."
diskutil unmountDisk "$DISK_DEV"

echo "Flashing... (this takes a few minutes)"
sudo dd if="$IMAGE" of="$RAW_DEV" bs=4m status=progress

echo ""
echo "Flushing buffers..."
sync

echo "Ejecting $DISK_DEV..."
diskutil eject "$DISK_DEV"

echo ""
echo "Done. SD card is ready."
echo "Insert into Orange Pi 5B and power on."
