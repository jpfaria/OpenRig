# Building OpenRig

How to build and run OpenRig from source. To just install it, see [Installing OpenRig](../user-guide/installation.md).

Builds work on macOS and Linux. The Windows source build does not work yet.

## Prerequisites

- **Rust** (stable), via [rustup](https://rustup.rs)
- **cmake** 3.16 or later
- **pkg-config**
- **Git** and **Git LFS** — LFS holds the vendored NeuralAmpModelerCore archive and the test audio

```bash
# macOS (plus the Xcode command line tools: xcode-select --install)
brew install cmake pkg-config git-lfs

# Debian / Ubuntu
sudo apt install build-essential git git-lfs cmake pkg-config libasound2-dev libfontconfig-dev

# Fedora
sudo dnf install gcc-c++ git git-lfs cmake pkg-config alsa-lib-devel fontconfig-devel
```

## Build and run

```bash
git lfs install
git clone https://github.com/jpfaria/OpenRig.git
cd OpenRig
cargo build --release -p adapter-gui
./target/release/adapter-gui
```

The NAM engine is C++ and is built by cargo itself: `crates/nam/build.rs` unpacks `deps/NeuralAmpModelerCore.tar.gz` and compiles it with cmake. There is no separate step.

## Plugins (models)

Amps, pedals, cabs, IRs and LV2 plugins live in [OpenRig-plugins](https://github.com/jpfaria/OpenRig-plugins), not in this repository. Without them the block picker is empty.

```bash
git clone https://github.com/jpfaria/OpenRig-plugins.git
```

Point OpenRig at `OpenRig-plugins/plugins/source`, either in **Settings → Paths → Plugins** or for one run:

```bash
OPENRIG_PLUGINS_ROOT=/path/to/OpenRig-plugins/plugins/source ./target/release/adapter-gui
```

Lookup order: `OPENRIG_PLUGINS_ROOT`, then the path set in Settings, then `plugins/` inside the app's data folder.

## Binaries

| Crate | Binary | What it is |
|---|---|---|
| `adapter-gui` | `adapter-gui` | The desktop app |
| `adapter-console` | `adapter-console` | Terminal frontend |
| `adapter-console-rig` | `adapter-console-rig` | Runs a project headless |
| `adapter-render` | `openrig-render` | Renders a chain offline to WAV — see [render](../render.md) |

Packages install the GUI as `openrig` and ship the other three next to it as `openrig-console`, `openrig-console-rig` and `openrig-render` (`scripts/lib/console-binaries.tsv`). `adapter-server` (gRPC) and `adapter-vst3` are reserved crates with no product yet. Command-line flags and environment variables: [cli.md](../cli.md).

## Packaging and installing a local build

```bash
OPENRIG_PLUGINS_DIR=/path/to/OpenRig-plugins/plugins/source ./scripts/install-macos-local.sh
```

This packages the current checkout as a `.dmg` (`scripts/package-macos.sh`) and installs it to `/Applications`. Every script is listed in [scripts.md](../scripts.md).

## LV2 plugin libraries

`scripts/build-lib.sh <plugin|all> [--platform linux-x86_64|linux-aarch64|windows-x64|all]` builds the native LV2 libraries that OpenRig-plugins ships; `--list` shows the plugins. Cross-platform builds run in Docker (`docker/Dockerfile.build-libs`). The app build does not need this.

## CI

| Workflow | What it does |
|---|---|
| `test.yml` | Tests on pushes and PRs to `develop`, `release/**`, `main` |
| `pr.yml` | The comparative quality gate — see [quality-gate.md](quality-gate.md) |
| `release.yml` | Builds the macOS `.dmg` and publishes the release on a tag — see [release.md](release.md) |
| `build-libs.yml` | Builds the LV2 plugin libraries |
| `pages.yml` | Publishes `site/` to GitHub Pages |
| `claude.yml` | Runs Claude on `@claude` issue comments |

## Dependencies

There are no git submodules. NeuralAmpModelerCore (with AudioDSPTools, Eigen and nlohmann) is vendored as `deps/NeuralAmpModelerCore.tar.gz` and pinned by `deps/NeuralAmpModelerCore.lock`; `crates/nam/build.rs` unpacks it into `deps/NeuralAmpModelerCore/` (not versioned) when that folder is missing or came from another lock. When upstream has a newer release, the first build after a `git fetch` prints a cargo warning; CI vendors it on `develop` after the tests pass. Every other build stays offline — see [deps/DEPS.md](../../deps/DEPS.md).

If a build fails with "is a Git LFS pointer", the clone was made without LFS: run `git lfs install && git lfs pull`.
