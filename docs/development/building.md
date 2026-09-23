# Building OpenRig

This guide covers how to build OpenRig from source for development and testing.

## Prerequisites

- **Rust toolchain** (stable) -- install via [rustup.rs](https://rustup.rs)
- **cmake** 3.16+
- **pkg-config**
- **Git**
- **Git LFS** (large binary assets -- NAM captures, IRs, and the vendored NeuralAmpModelerCore sources)

## Quick Build (GUI only)

```bash
git lfs install
git clone https://github.com/jpfaria/OpenRig.git
cd OpenRig
cargo build --release -p adapter-gui
```

Binary output: `target/release/adapter-gui`

## Platform-Specific Dependencies

### macOS

```bash
brew install cmake pkg-config
```

### Ubuntu / Debian

```bash
sudo apt install build-essential cmake pkg-config libasound2-dev libfontconfig-dev
```

### Fedora

```bash
sudo dnf install cmake pkg-config alsa-lib-devel fontconfig-devel
```

### Windows

- Install Visual Studio Build Tools (C++ workload)
- Install cmake (add to PATH)

## Build Targets

| Target | Command | Description |
|--------|---------|-------------|
| Desktop GUI | `cargo build -p adapter-gui` | Slint-based desktop application |
| Console | `cargo build -p adapter-console` | CLI interface |
| Server | `cargo build -p adapter-server` | gRPC remote control server |
| VST3 Plugin | `cargo build -p adapter-vst3` | VST3/AU plugin for DAWs |

## NAM Engine (C++/CMake)

The Neural Amp Modeler engine is a C++ library that must be compiled separately:

```bash
./scripts/build-lib.sh nam
```

This builds the NAM shared library for your current platform. The script handles cmake configuration and compilation.

For specific platforms:

```bash
./scripts/build-lib.sh nam --platform linux-x86_64
./scripts/build-lib.sh nam --platform linux-arm64
./scripts/build-lib.sh nam --platform windows-x64
./scripts/build-lib.sh all --platform all  # Build everything for all platforms
```

Prebuilt libraries are available on the releases page for convenience.

## Docker Cross-Compilation

For building native libraries across all 5 platforms:

```bash
docker build -f docker/Dockerfile.build-libs -t openrig-build .
```

The Docker image (based on Ubuntu 22.04) includes:

- **Build tools:** gcc, cmake, meson, ninja, autoconf
- **Audio/LV2:** lv2-dev, libsndfile1-dev, libsamplerate0-dev, libfftw3-dev
- **Cross-compilation:** mingw-w64, llvm-mingw for Windows targets

## CI/CD (GitHub Actions)

Two workflows:

- **build-libs.yml** -- Builds native C++ libraries for all platforms
- **claude.yml** -- AI-assisted code review on issue comments

## Dependencies

OpenRig has no git submodules. The one C++ dependency the app build needs,
NeuralAmpModelerCore (with AudioDSPTools, Eigen and nlohmann inside it), is
vendored as `deps/NeuralAmpModelerCore.tar.gz` (Git LFS) and pinned by
`deps/NeuralAmpModelerCore.lock`. `crates/nam/build.rs` unpacks it into
`deps/NeuralAmpModelerCore/` (not versioned) whenever that folder is missing or
came from another lock, so a build never touches the network (#974). Updating
it is `python3 scripts/nam_vendor.py update` — see [deps/DEPS.md](../../deps/DEPS.md).

Key workspace dependencies (Cargo.toml):

- **slint** -- UI framework
- **cpal** -- Cross-platform audio I/O
- **tokio** -- Async runtime
- **tonic/prost** -- gRPC (server mode)
- **serde/serde_yaml** -- Serialization
- **anyhow/thiserror** -- Error handling

## Git LFS

Large binary assets (NAM captures, IR files, the vendored NeuralAmpModelerCore archive) are tracked with Git LFS. Ensure LFS is installed:

```bash
git lfs install
git lfs pull
```
