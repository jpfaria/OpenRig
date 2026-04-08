#!/usr/bin/env bash
# Set up and launch a UTM ARM64 VM to validate the OpenRig image.
#
# On first boot the VM automatically:
#   - Installs runtime dependencies
#   - Downloads the latest OpenRig linux-aarch64 release from GitHub
#   - Configures ALSA and Slint linuxkms environment
#   - Creates /usr/local/bin/openrig-start helper
#
# After the VM boots, log in as: openrig / openrig
# Then run: openrig-start
#
# Usage:
#   ./scripts/validate-utm.sh               # set up and launch VM
#   ./scripts/validate-utm.sh --reinstall   # delete existing VM and recreate
#
# Prerequisites (auto-installed):
#   - UTM 4.x (brew install --cask utm)
#   - qemu  (brew install qemu) — for qemu-img

set -euo pipefail

VM_NAME="OpenRig-Validation"
UBUNTU_URL="https://cloud-images.ubuntu.com/releases/22.04/release/ubuntu-22.04-server-cloudimg-arm64.img"
DISK_SIZE="16G"
VM_MEMORY=4096   # MiB
VM_CPUS=4

WORK_DIR="${HOME}/.openrig-utm"
DISK_IMG="$WORK_DIR/disk0.qcow2"
SEED_ISO="$WORK_DIR/seed.iso"

REINSTALL=false
for arg in "$@"; do
    case "$arg" in
        --reinstall) REINSTALL=true ;;
        --help|-h)
            grep '^#' "$0" | head -20 | sed 's/^# //'
            exit 0
            ;;
        *)
            echo "Unknown argument: $arg"
            exit 1
            ;;
    esac
done

# ── Helpers ───────────────────────────────────────────────────────────────────
step() {
    echo ""
    echo "══════════════════════════════════════════"
    echo "  $*"
    echo "══════════════════════════════════════════"
}

# ── Check Apple Silicon ───────────────────────────────────────────────────────
check_prereqs() {
    if [ "$(uname)" != "Darwin" ]; then
        echo "ERROR: This script requires macOS."
        exit 1
    fi

    ARCH=$(uname -m)
    if [ "$ARCH" != "arm64" ]; then
        echo "WARNING: Apple Silicon recommended for native ARM64 speed."
        echo "         Intel Mac will use QEMU software emulation (slow)."
        read -rp "Continue anyway? (y/N): " CONT
        [ "$CONT" = "y" ] || exit 0
    fi

    command -v brew >/dev/null || {
        echo "ERROR: Homebrew not found. Install from https://brew.sh"
        exit 1
    }
}

# ── Install dependencies ──────────────────────────────────────────────────────
install_deps() {
    step "Installing dependencies"

    if ! command -v qemu-img >/dev/null 2>&1; then
        echo "  Installing qemu..."
        brew install qemu
    else
        echo "  qemu: OK"
    fi

    if ! osascript -e 'id of app "UTM"' >/dev/null 2>&1; then
        echo "  Installing UTM..."
        brew install --cask utm
    else
        echo "  UTM: OK"
    fi
}

# ── Find UTM documents folder ─────────────────────────────────────────────────
utm_documents_dir() {
    # Mac App Store version
    local mas_dir="$HOME/Library/Containers/com.utmapp.UTM/Data/Documents"
    # Direct download / Homebrew Cask version
    local direct_dir="$HOME/Documents/UTM"

    if [ -d "$mas_dir" ]; then
        echo "$mas_dir"
    else
        mkdir -p "$direct_dir"
        echo "$direct_dir"
    fi
}

# ── Download and prepare disk image ──────────────────────────────────────────
prepare_disk() {
    step "Preparing disk image"
    mkdir -p "$WORK_DIR"

    local raw_img="$WORK_DIR/ubuntu-arm64.img"

    if [ ! -f "$raw_img" ]; then
        echo "  Downloading Ubuntu 22.04 ARM64 cloud image..."
        curl -L --progress-bar "$UBUNTU_URL" -o "$raw_img"
    else
        echo "  Ubuntu image: cached"
    fi

    echo "  Converting to qcow2 and resizing to $DISK_SIZE..."
    qemu-img convert -f qcow2 -O qcow2 "$raw_img" "$DISK_IMG"
    qemu-img resize "$DISK_IMG" "$DISK_SIZE"
    echo "  Disk: $DISK_IMG"
}

# ── Create cloud-init seed ISO ────────────────────────────────────────────────
create_seed_iso() {
    step "Creating cloud-init seed ISO"

    local seed_dir="$WORK_DIR/seed"
    mkdir -p "$seed_dir"

    # meta-data
    cat > "$seed_dir/meta-data" <<'EOF'
instance-id: openrig-validation
local-hostname: openrig-vm
EOF

    # user-data
    cat > "$seed_dir/user-data" <<'USERDATA'
#cloud-config
hostname: openrig-vm

users:
  - name: openrig
    gecos: OpenRig
    sudo: ALL=(ALL) NOPASSWD:ALL
    shell: /bin/bash
    lock_passwd: false
    plain_text_passwd: "openrig"

chpasswd:
  expire: false

ssh_pwauth: true

package_update: true
package_upgrade: false

packages:
  - alsa-utils
  - libfreetype6
  - libfontconfig1
  - libdrm2
  - libgles2
  - libegl1
  - gh
  - curl
  - xz-utils

runcmd:
  # Download latest OpenRig release for linux-aarch64
  - |
    mkdir -p /opt/openrig-release
    gh release download \
      --repo jpfaria/OpenRig \
      --pattern "openrig-*-linux-aarch64.tar.gz" \
      --dir /opt/openrig-release \
      --clobber 2>/dev/null || \
    curl -s https://api.github.com/repos/jpfaria/OpenRig/releases/latest \
      | grep "browser_download_url.*linux-aarch64.tar.gz" \
      | cut -d '"' -f 4 \
      | xargs curl -L -o /opt/openrig-release/openrig-linux-aarch64.tar.gz
  - |
    cd /opt/openrig-release
    tar -xzf openrig-*-linux-aarch64.tar.gz
    RELEASE_DIR=$(ls -d openrig-*-linux-aarch64 | head -1)
    install -m 755 "$RELEASE_DIR/openrig" /usr/local/bin/openrig
    mkdir -p /usr/local/lib/openrig /usr/local/share/openrig
    cp -r "$RELEASE_DIR/libs"   /usr/local/lib/openrig/
    cp -r "$RELEASE_DIR/data"   /usr/local/share/openrig/
    cp -r "$RELEASE_DIR/assets" /usr/local/share/openrig/
  # openrig-start helper
  - |
    cat > /usr/local/bin/openrig-start <<'EOF'
    #!/bin/bash
    export SLINT_BACKEND=linuxkms
    export SLINT_RENDERER=software
    export RUST_LOG=warn
    exec /usr/local/bin/openrig "$@"
    EOF
    chmod +x /usr/local/bin/openrig-start
  # Add openrig user to audio/video groups
  - usermod -aG audio,video openrig

final_message: |
  OpenRig VM ready. Log in as openrig/openrig and run: openrig-start
USERDATA

    echo "  Creating seed ISO..."
    hdiutil makehybrid \
        -o "$SEED_ISO" \
        -hfs -iso -joliet \
        -default-volume-name "cidata" \
        "$seed_dir" \
        -quiet
    echo "  Seed: $SEED_ISO"
}

# ── Create UTM VM bundle ──────────────────────────────────────────────────────
create_utm_vm() {
    step "Creating UTM VM: $VM_NAME"

    local utm_docs
    utm_docs=$(utm_documents_dir)
    local vm_dir="$utm_docs/${VM_NAME}.utm"

    if [ -d "$vm_dir" ]; then
        if $REINSTALL; then
            echo "  Removing existing VM..."
            rm -rf "$vm_dir"
        else
            echo "  VM already exists at: $vm_dir"
            echo "  Use --reinstall to recreate it."
            return
        fi
    fi

    mkdir -p "$vm_dir/Images"
    cp "$DISK_IMG" "$vm_dir/Images/disk0.qcow2"
    cp "$SEED_ISO" "$vm_dir/Images/seed.iso"

    local vm_uuid
    vm_uuid=$(python3 -c "import uuid; print(str(uuid.uuid4()).upper())")

    # Generate config.plist (Apple Virtualization Framework backend)
    python3 - "$vm_dir/config.plist" "$vm_uuid" "$VM_NAME" "$VM_MEMORY" "$VM_CPUS" <<'PYEOF'
import sys
import plistlib

out_path, vm_uuid, vm_name, memory, cpus = sys.argv[1], sys.argv[2], sys.argv[3], int(sys.argv[4]), int(sys.argv[5])

config = {
    "Backend": "Apple",
    "ConfigurationVersion": 5,
    "Information": {
        "Name": vm_name,
        "UUID": vm_uuid,
        "IconCustom": False,
        "Notes": "OpenRig ARM64 validation VM"
    },
    "Apple": {
        "BootLoader": {
            "OperatingSystem": "Linux",
            "UEFI": True
        },
        "CPUCount": cpus,
        "MemorySizeMiB": memory,
        "StorageDevices": [
            {
                "ImagePath": "disk0.qcow2",
                "IsReadOnly": False,
                "InterfaceType": "VirtIO",
                "DeviceType": "Disk"
            },
            {
                "ImagePath": "seed.iso",
                "IsReadOnly": True,
                "InterfaceType": "USB",
                "DeviceType": "CDROM"
            }
        ],
        "NetworkDevices": [
            {
                "NetworkMode": "Shared"
            }
        ],
        "DisplayDevices": [
            {
                "DeviceType": "VirtioGPU",
                "IsDynamicResolution": True
            }
        ],
        "AudioDevices": [
            {
                "DeviceType": "VirtioSound"
            }
        ],
        "ClipboardSharing": True,
        "BalloonDevice": True,
        "EntropyDevice": True,
        "KeyboardDevice": "USB",
        "PointerDevice": "USB"
    }
}

with open(out_path, "wb") as f:
    plistlib.dump(config, f, fmt=plistlib.FMT_XML)

print(f"  config.plist written: {out_path}")
PYEOF

    echo ""
    echo "  VM bundle: $vm_dir"
}

# ── Launch UTM ────────────────────────────────────────────────────────────────
launch_utm() {
    step "Launching UTM"
    open -a UTM
    echo ""
    echo "  UTM is open. Your VM '$VM_NAME' should appear in the sidebar."
    echo "  If it doesn't: File → Open → select $(utm_documents_dir)/${VM_NAME}.utm"
    echo ""
    echo "  First boot takes ~2-3 minutes (cloud-init installs OpenRig)."
    echo ""
    echo "  When ready:"
    echo "    Login:    openrig / openrig"
    echo "    Run UI:   openrig-start"
}

# ── Main ──────────────────────────────────────────────────────────────────────
echo "OpenRig — UTM Validation VM Setup"
echo "VM:     $VM_NAME (ARM64, ${VM_MEMORY}MiB, ${VM_CPUS} CPUs)"
echo ""

check_prereqs
install_deps
prepare_disk
create_seed_iso
create_utm_vm
launch_utm

echo "Done."
