#!/usr/bin/env bash
# Set up and launch a QEMU ARM64 VM to validate OpenRig on Linux.
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
#   ./scripts/validate-qemu.sh               # set up and launch VM
#   ./scripts/validate-qemu.sh --reinstall   # delete and recreate
#   ./scripts/validate-qemu.sh --no-kvm      # force software emulation
#
# Prerequisites (auto-installed via apt):
#   - qemu-system-aarch64, qemu-utils, qemu-efi-aarch64, genisoimage

set -euo pipefail

UBUNTU_URL="https://cloud-images.ubuntu.com/releases/22.04/release/ubuntu-22.04-server-cloudimg-arm64.img"
DISK_SIZE="16G"
VM_MEMORY=4096   # MiB
VM_CPUS=4

WORK_DIR="${HOME}/.openrig-qemu"
DISK_IMG="$WORK_DIR/disk0.qcow2"
SEED_ISO="$WORK_DIR/seed.iso"

REINSTALL=false
NO_KVM=false

while [ $# -gt 0 ]; do
    case "$1" in
        --reinstall) REINSTALL=true  ; shift ;;
        --no-kvm)    NO_KVM=true     ; shift ;;
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
step() {
    echo ""
    echo "══════════════════════════════════════════"
    echo "  $*"
    echo "══════════════════════════════════════════"
}

# ── Check platform ────────────────────────────────────────────────────────────
check_prereqs() {
    if [ "$(uname)" != "Linux" ]; then
        echo "ERROR: This script requires Linux. On macOS use validate-utm.sh."
        exit 1
    fi
}

# ── Install dependencies ──────────────────────────────────────────────────────
install_deps() {
    step "Installing dependencies"

    local pkgs=()
    command -v qemu-system-aarch64 >/dev/null 2>&1 || pkgs+=(qemu-system-aarch64)
    command -v qemu-img            >/dev/null 2>&1 || pkgs+=(qemu-utils)
    command -v genisoimage         >/dev/null 2>&1 || pkgs+=(genisoimage)

    local efi_fw
    efi_fw=$(find /usr/share -name "QEMU_EFI.fd" 2>/dev/null | head -1 || true)
    [ -n "$efi_fw" ] || pkgs+=(qemu-efi-aarch64)

    if [ ${#pkgs[@]} -gt 0 ]; then
        echo "  Installing: ${pkgs[*]}"
        sudo apt-get update -qq
        sudo apt-get install -y "${pkgs[@]}"
    else
        echo "  All dependencies OK"
    fi
}

# ── Prepare disk image ────────────────────────────────────────────────────────
prepare_disk() {
    step "Preparing disk image"
    mkdir -p "$WORK_DIR"

    if $REINSTALL; then
        echo "  Removing existing VM data..."
        rm -f "$DISK_IMG" "$SEED_ISO"
    fi

    local raw_img="$WORK_DIR/ubuntu-arm64.img"
    if [ ! -f "$raw_img" ]; then
        echo "  Downloading Ubuntu 22.04 ARM64 cloud image..."
        curl -L --progress-bar "$UBUNTU_URL" -o "$raw_img"
    else
        echo "  Ubuntu image: cached"
    fi

    if [ ! -f "$DISK_IMG" ]; then
        echo "  Converting to qcow2 and resizing to $DISK_SIZE..."
        qemu-img convert -f qcow2 -O qcow2 "$raw_img" "$DISK_IMG"
        qemu-img resize "$DISK_IMG" "$DISK_SIZE"
        echo "  Disk: $DISK_IMG"
    else
        echo "  Disk image: exists"
    fi
}

# ── Create cloud-init seed ISO ────────────────────────────────────────────────
create_seed_iso() {
    step "Creating cloud-init seed ISO"

    if [ -f "$SEED_ISO" ]; then
        echo "  Seed ISO: exists"
        return
    fi

    local seed_dir="$WORK_DIR/seed"
    mkdir -p "$seed_dir"

    cat > "$seed_dir/meta-data" <<'EOF'
instance-id: openrig-validation
local-hostname: openrig-vm
EOF

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
  - curl
  - tar

write_files:
  - path: /usr/local/bin/openrig-start
    permissions: '0755'
    content: |
      #!/bin/bash
      export SLINT_BACKEND=linuxkms
      export SLINT_RENDERER=software
      export RUST_LOG=warn
      exec /usr/local/bin/openrig "$@"

runcmd:
  - |
    LATEST=$(curl -sf https://api.github.com/repos/jpfaria/OpenRig/releases/latest \
      | grep "browser_download_url.*linux-aarch64\.tar\.gz" \
      | cut -d '"' -f 4)
    mkdir -p /opt/openrig-release
    curl -L "$LATEST" -o /opt/openrig-release/openrig-linux-aarch64.tar.gz
  - |
    cd /opt/openrig-release
    tar -xzf openrig-*-linux-aarch64.tar.gz
    RELEASE_DIR=$(ls -d openrig-*-linux-aarch64 2>/dev/null | head -1)
    install -m 755 "$RELEASE_DIR/openrig" /usr/local/bin/openrig
    mkdir -p /usr/local/share/openrig
    cp -r "$RELEASE_DIR/libs"     /usr/local/share/openrig/
    cp -r "$RELEASE_DIR/data"     /usr/local/share/openrig/
    cp -r "$RELEASE_DIR/assets"   /usr/local/share/openrig/
    cp -r "$RELEASE_DIR/captures" /usr/local/share/openrig/ 2>/dev/null || true
  - usermod -aG audio,video openrig

final_message: |
  OpenRig VM ready. Log in as openrig/openrig and run: openrig-start
USERDATA

    echo "  Creating seed ISO..."
    genisoimage \
        -output "$SEED_ISO" \
        -volid "cidata" \
        -joliet -rock \
        -quiet \
        "$seed_dir"
    echo "  Seed: $SEED_ISO"
}

# ── Launch QEMU ───────────────────────────────────────────────────────────────
launch_qemu() {
    step "Launching QEMU ARM64 VM"

    # Locate UEFI firmware
    local efi_fw
    efi_fw=$(find /usr/share -name "QEMU_EFI.fd" 2>/dev/null | head -1)
    if [ -z "$efi_fw" ]; then
        echo "ERROR: QEMU_EFI.fd not found. Install qemu-efi-aarch64."
        exit 1
    fi

    # KVM: only useful when host is also aarch64
    local kvm_args=()
    if ! $NO_KVM && [ -e /dev/kvm ] && [ "$(uname -m)" = "aarch64" ]; then
        kvm_args=(-enable-kvm -cpu host)
        echo "  Mode: KVM (native aarch64 — fast)"
    else
        kvm_args=(-cpu cortex-a57)
        echo "  Mode: software emulation (slow on x86_64, ~10x slower than native)"
    fi

    # Audio: PulseAudio preferred, fallback to ALSA
    local audio_args=()
    if command -v pactl >/dev/null 2>&1; then
        audio_args=(-audiodev pa,id=snd0 -device virtio-sound-pci,audiodev=snd0)
        echo "  Audio: PulseAudio"
    else
        audio_args=(-audiodev alsa,id=snd0 -device virtio-sound-pci,audiodev=snd0)
        echo "  Audio: ALSA"
    fi

    echo "  Memory: ${VM_MEMORY}MiB | CPUs: ${VM_CPUS}"
    echo ""
    echo "  First boot: ~2-3 min (cloud-init installs OpenRig)"
    echo "  Login:      openrig / openrig"
    echo "  Run UI:     openrig-start"
    echo ""

    qemu-system-aarch64 \
        -machine virt,gic-version=3 \
        "${kvm_args[@]}" \
        -m "${VM_MEMORY}M" \
        -smp "${VM_CPUS}" \
        -bios "$efi_fw" \
        -drive "if=none,file=${DISK_IMG},id=hd0,format=qcow2" \
        -device virtio-blk-pci,drive=hd0 \
        -drive "if=none,file=${SEED_ISO},id=cdrom,format=raw,readonly=on" \
        -device usb-storage,drive=cdrom \
        -device usb-ehci \
        -netdev user,id=net0 \
        -device virtio-net-pci,netdev=net0 \
        -device virtio-gpu-pci \
        -display gtk,full-screen=off \
        "${audio_args[@]}" \
        -device virtio-keyboard-pci \
        -device virtio-mouse-pci \
        -serial mon:stdio
}

# ── Main ──────────────────────────────────────────────────────────────────────
echo "OpenRig — QEMU ARM64 Validation VM (Linux)"
echo "Memory: ${VM_MEMORY}MiB | CPUs: ${VM_CPUS} | Disk: ${DISK_SIZE}"
echo ""

check_prereqs
install_deps
prepare_disk
create_seed_iso
launch_qemu

echo "Done."
