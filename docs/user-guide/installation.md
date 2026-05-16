# Installation Guide

This guide covers how to install and run OpenRig on your system, either from prebuilt binaries or by building from source.

## System Requirements

| Requirement | Details |
|-------------|---------|
| **macOS** | macOS 12 or later (Apple Silicon or Intel) |
| **Linux** | x86_64 or aarch64 with ALSA support |
| **Windows** | Windows 10 or later (x86_64) |
| **RAM** | Minimum 4 GB |
| **Disk** | Minimum 500 MB free space |

**Recommended:** An audio interface such as a Focusrite Scarlett is strongly recommended for achieving low-latency performance. Built-in audio devices may introduce noticeable latency during live use.

## Download Binaries

Prebuilt binaries for every supported platform are published on the [latest release page](https://github.com/jpfaria/OpenRig/releases/latest). Pick the artifact for your OS and architecture (`x86_64` for most desktops/laptops, `aarch64` for ARM boards such as the Orange Pi).

### Linux

Two ways to install — pick one.

**AppImage (recommended — no root, self-contained, nothing installed system-wide):**

```bash
# x86_64 — replace <ver> with the release tag, e.g. 0.1.0-dev.19
chmod +x OpenRig-<ver>-linux-x86_64.AppImage
./OpenRig-<ver>-linux-x86_64.AppImage
```

To remove it, just delete the file. Use the `-linux-aarch64.AppImage` asset on ARM boards.

**`.deb` / `.rpm` (system-integrated — adds a desktop entry and resolves dependencies, requires root):**

```bash
# Ubuntu / Debian, x86_64 (use openrig_<ver>_arm64.deb on ARM)
sudo apt install ./openrig_<ver>_amd64.deb

# Fedora / RHEL, x86_64 (use the .aarch64.rpm on ARM)
sudo dnf install ./openrig-<ver>-1.x86_64.rpm
```

Prefer `apt install ./file.deb` over `dpkg -i` — it pulls in dependencies automatically.

A portable `openrig-<ver>-linux-<arch>.tar.gz` is also published: extract it and run the `adapter-gui` binary directly.

#### Audio setup (required for sound)

Installing the package is not enough to get sound — OpenRig is a guitar
pedalboard and needs a real audio I/O path:

1. **A USB audio interface** with a guitar input and an output
   (headphones/monitors). Class-compliant interfaces work on Linux with
   no driver. The built-in laptop audio is not enough (no instrument
   input). Verify it is seen: `arecord -l` / `aplay -l` or
   `cat /proc/asound/cards`.
2. **JACK server** — OpenRig launches `jackd` itself, so the daemon
   (not just the libraries) must be installed:
   ```bash
   sudo apt install jackd2          # Debian / Ubuntu
   sudo dnf install jack-audio-connection-kit   # Fedora
   ```
3. **Audio group** — add your user and re-login (so realtime/device
   access applies):
   ```bash
   sudo usermod -aG audio "$USER"
   ```
4. **PipeWire / PulseAudio coexistence** — if a sound server is holding
   the interface, OpenRig's `jackd` may fail to grab it exclusively.
   Either suspend the device in the sound server or point OpenRig at the
   PipeWire JACK/ALSA bridge.
5. In OpenRig's **audio screen**, select the interface as input and
   output, set sample rate / buffer size, then enable the chain.

The `.deb` declares `libasound2` and `libseat1`; `jackd2` is recommended
separately because some setups route audio through PipeWire's JACK
bridge instead.

### macOS

Download `OpenRig-<ver>-macos-universal.dmg`, open it, and drag OpenRig to Applications. The build is universal (Apple Silicon + Intel).

### Windows

Run the `OpenRig-<ver>-windows-x64.msi` installer, or download `OpenRig-<ver>-windows-x64.zip` for a portable copy and run the `adapter-gui` executable.

### macOS — one-line install (recommended)

The macOS build is ad-hoc signed but **not** Apple-notarized (no paid
Developer certificate). A browser/Finder download tags it with
`com.apple.quarantine`, so double-clicking the `.dmg` can show
*"OpenRig is damaged and can't be opened"*. That message is misleading —
the app is fine, Gatekeeper just blocks un-notarized downloads.

Install with one command (fetched via `curl`, which does not quarantine,
so it runs without the block):

```bash
curl -fsSL https://raw.githubusercontent.com/jpfaria/OpenRig/develop/scripts/install-macos.sh | bash
```

It downloads the latest release `.dmg`, copies `OpenRig.app` to
`/Applications`, and strips the quarantine attribute. Pin a version with
`| bash -s -- v0.1.0-dev.19`.

If you installed the `.dmg` manually instead, clear the flag yourself:

```bash
xattr -dr com.apple.quarantine /Applications/OpenRig.app
```

## Build from Source

### Prerequisites

- **Rust toolchain** -- install via [rustup.rs](https://rustup.rs)
- **cmake** (version 3.16 or later)
- **pkg-config**

### Platform-Specific Dependencies

**macOS:**

```bash
brew install cmake pkg-config
```

**Ubuntu / Debian:**

```bash
sudo apt install cmake pkg-config libasound2-dev libfontconfig-dev
```

**Fedora:**

```bash
sudo dnf install cmake pkg-config alsa-lib-devel fontconfig-devel
```

**Windows:**

Install [Visual Studio Build Tools](https://visualstudio.microsoft.com/visual-cpp-build-tools/) with the C++ workload, then install [cmake](https://cmake.org/download/) and ensure it is available on your PATH.

### Build Steps

Clone the repository and build the release binary:

```bash
git clone https://github.com/jpfaria/OpenRig.git
cd OpenRig
git submodule update --init --recursive
cargo build --release -p adapter-gui
```

The compiled binary will be located at `target/release/adapter-gui`.

## NAM Engine (Optional)

For [Neural Amp Modeler](https://www.neuralampmodeler.com/) support, the NAM C++ engine must be built separately:

```bash
./scripts/build-lib.sh nam
```

Alternatively, prebuilt NAM libraries are available on the [releases page](https://github.com/jpfaria/OpenRig/releases/latest). Download the appropriate library for your platform and place it in the expected path.

## Docker Cross-Compilation

To build libraries for all supported platforms using Docker:

```bash
docker build -f docker/Dockerfile.build-libs -t openrig-build .
./scripts/build-lib.sh all --platform all
```

This is primarily intended for CI and release workflows, but can also be used for local cross-compilation.

## Troubleshooting

### "OpenRig is damaged and can't be opened" (macOS)

The app is not damaged. macOS shows this for downloads that are not
Apple-notarized. Use the one-line installer, or strip the quarantine
flag manually — see [macOS — one-line install](#macos--one-line-install-recommended)
above. Right-click → *Open* also works once the app is a valid bundle.

### "cannot open shared object file" on Linux

Builds from `v0.1.0-dev.20` onward bundle `libNeuralAudioCAPI.so` (via
RUNPATH) and the `.deb` declares `libseat1` as a dependency, so this is
handled automatically. On older builds the app fails to start with
`error while loading shared libraries: libNeuralAudioCAPI.so` or
`libseat.so.1`. Fix by upgrading, or for `libseat`:

```bash
sudo apt install libseat1   # Debian / Ubuntu
```

### ALSA not found (Linux)

If you encounter errors related to ALSA during compilation on Linux, install the ALSA development headers:

```bash
# Debian / Ubuntu
sudo apt install libasound2-dev

# Fedora
sudo dnf install alsa-lib-devel
```

### cmake version too old

OpenRig requires cmake 3.16 or later. Check your installed version with:

```bash
cmake --version
```

If your distribution ships an older version, install a newer one from the [cmake downloads page](https://cmake.org/download/) or use a package manager such as `snap` or `pip`.

### Linker errors on Windows

Ensure that [Visual Studio Build Tools](https://visualstudio.microsoft.com/visual-cpp-build-tools/) are installed with the **Desktop development with C++** workload selected. Restart your terminal after installation.

### Submodule errors

If the build fails due to missing submodule files, re-initialize and update all submodules:

```bash
git submodule update --init --recursive
```
