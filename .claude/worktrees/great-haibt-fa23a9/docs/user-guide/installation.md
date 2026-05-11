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

Prebuilt binaries are available for all supported platforms. Download the appropriate archive from the latest release page.

| Platform | Architecture | Download |
|----------|--------------|----------|
| macOS | Apple Silicon (aarch64) | [Latest Release](https://github.com/jpfaria/OpenRig/releases/latest) |
| macOS | Intel (x86_64) | [Latest Release](https://github.com/jpfaria/OpenRig/releases/latest) |
| Linux | x86_64 | [Latest Release](https://github.com/jpfaria/OpenRig/releases/latest) |
| Linux | aarch64 | [Latest Release](https://github.com/jpfaria/OpenRig/releases/latest) |
| Windows | x86_64 | [Latest Release](https://github.com/jpfaria/OpenRig/releases/latest) |

After downloading, extract the archive and run the `adapter-gui` binary for your platform.

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
