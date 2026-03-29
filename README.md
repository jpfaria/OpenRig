<p align="center">
  <img src="docs/assets/openrig-logo.svg" alt="OpenRig" width="320">
</p>

<p align="center">
  <strong>Build your rig once. Use it everywhere.</strong>
</p>

<p align="center">
  <a href="LICENSE"><img src="https://img.shields.io/badge/license-GPL--3.0-blue.svg" alt="License: GPL-3.0"></a>
  <img src="https://img.shields.io/badge/version-0.1.0-orange.svg" alt="Version: 0.1.0">
  <img src="https://img.shields.io/badge/rust-2021_edition-brightgreen.svg" alt="Rust: 2021 Edition">
  <img src="https://img.shields.io/badge/platforms-macOS%20%7C%20Linux%20%7C%20Windows-lightgrey.svg" alt="Platforms: macOS | Linux | Windows">
</p>

---

## What is OpenRig?

OpenRig is a virtual pedalboard and rig platform for musicians. Build signal chains with amp models, effects, cabinets, and more — all processed in real time with professional-grade audio quality.

Built in Rust for performance and reliability, OpenRig combines four audio backends — Native DSP, Neural Amp Modeler (NAM), Impulse Response (IR), and LV2 plugins — into a single platform that runs on macOS, Linux, and Windows. The long-term vision: one rig that works as a standalone app, a VST3 plugin in your DAW, a server for remote setups, and dedicated hardware on stage.

## Features

- **174 amp and effect models** across 14 block types — preamps, amps, cabinets, gain pedals, delays, reverbs, modulation, dynamics, filters, wah, tuner, acoustic body resonance, and full rigs
- **4 audio backends** — Native Rust DSP, Neural Amp Modeler (NAM), Impulse Response (IR), and LV2 open-source plugins
- **Chain-based signal routing** — build your signal path visually by adding, removing, and reordering blocks
- **Real-time parameter control** — adjust knobs, sliders, and switches with instant audio feedback
- **Multiple I/O support** — multiple input and output blocks per chain, each with independent device and channel configuration
- **Per-chain instrument filtering** — electric guitar, acoustic guitar, bass, voice, keys, drums, or generic
- **5 platform targets** — macOS (Apple Silicon + Intel), Linux (x86_64 + aarch64), Windows (x86_64)
- **In-app feedback** — report bugs and suggestions directly from the interface

## Block Types

| Type | Models | Backends | Description |
|------|-------:|----------|-------------|
| **Preamp** | 5 | Native, NAM | Pre-amplification with gain and EQ |
| **Amp** | 12 | Native, NAM | Full amplifier (preamp + power amp) |
| **Cab** | 11 | Native, IR | Speaker cabinet simulation |
| **Gain** | 13 | Native, NAM, LV2 | Overdrive, distortion, fuzz, boost |
| **Delay** | 6 | Native | Echo and temporal repetition |
| **Reverb** | 1 | Native | Ambience and space simulation |
| **Modulation** | 5 | Native | Chorus, tremolo, vibrato |
| **Dynamics** | 2 | Native | Compressor and noise gate |
| **Filter** | 1 | Native | EQ and tonal shaping |
| **Wah** | 1 | Native | Wah-wah pedal |
| **Utility** | 1 | Native | Chromatic tuner |
| **Body** | 114 | IR | Acoustic body resonance (Taylor, Martin, Gibson, and more) |
| **Full Rig** | 1 | NAM | All-in-one amp with effects |
| **I/O** | — | — | Input and output routing blocks |

See the complete [Blocks Reference](docs/user-guide/blocks-reference.md) for all 174 models with parameters and ranges.

## Installation

### Download

| Platform | Architecture | Download |
|----------|-------------|----------|
| macOS | Apple Silicon (aarch64) | [Latest Release](https://github.com/jpfaria/OpenRig/releases/latest) |
| macOS | Intel (x86_64) | [Latest Release](https://github.com/jpfaria/OpenRig/releases/latest) |
| Linux | x86_64 | [Latest Release](https://github.com/jpfaria/OpenRig/releases/latest) |
| Linux | aarch64 | [Latest Release](https://github.com/jpfaria/OpenRig/releases/latest) |
| Windows | x86_64 | [Latest Release](https://github.com/jpfaria/OpenRig/releases/latest) |

### Build from Source

```bash
git clone https://github.com/jpfaria/OpenRig.git
cd OpenRig
git submodule update --init --recursive
cargo build --release -p adapter-gui
```

See the [Installation Guide](docs/user-guide/installation.md) for detailed instructions, platform-specific dependencies, and troubleshooting.

## Quick Start

1. **Launch OpenRig** and create a new project
2. **Configure audio** — select your input device (guitar interface) and output device (headphones/monitors)
3. **Build a chain** — add blocks between Input and Output: Preamp → Cab → Delay → Reverb
4. **Adjust parameters** — click any block to open its editor, tweak knobs in real time
5. **Play** — your signal chain is processing live audio

See the [Quick Start Guide](docs/user-guide/quick-start.md) for a complete walkthrough.

## Architecture

```
┌──────────────────────────────────────────────────────────┐
│   Adapters — GUI (Slint), Console, Server (gRPC), VST3   │
├──────────────────────────────────────────────────────────┤
│   Engine — real-time audio graph, block chain processing  │
├──────────────────────────────────────────────────────────┤
│   Application — orchestration, validation, commands       │
├──────────────────────────────────────────────────────────┤
│   Domain — blocks, chains, parameters, value objects      │
└──────────────────────────────────────────────────────────┘
```

OpenRig is organized as a Rust workspace with **38 crates** in five groups: core domain (5), audio processing blocks (17), backend integration (4), adapters and UI (6), and infrastructure (5). The architecture follows a clean/hexagonal pattern — inner layers define interfaces, outer layers implement them.

See [Architecture](docs/development/architecture.md) for the full crate map, audio signal flow, and design patterns.

## Documentation

### For Musicians

- [Installation Guide](docs/user-guide/installation.md) — download, build, and set up OpenRig
- [Quick Start](docs/user-guide/quick-start.md) — create your first project and signal chain
- [Blocks Reference](docs/user-guide/blocks-reference.md) — all 174 models with parameters
- [Presets](docs/user-guide/presets.md) — create, save, and share chain configurations

### For Developers

- [Architecture](docs/development/architecture.md) — crate map, layers, and design patterns
- [Building](docs/development/building.md) — full build guide including NAM engine and Docker
- [Creating Blocks](docs/development/creating-blocks.md) — how to add new audio models
- [Audio Backends](docs/development/audio-backends.md) — Native, NAM, IR, and LV2 internals

## Contributing

OpenRig welcomes contributions. We follow [Gitflow](https://nvie.com/posts/a-successful-git-branching-model/) with strict code quality standards — zero warnings, zero coupling, single source of truth.

See [CONTRIBUTING.md](CONTRIBUTING.md) for the complete guide on branching, commits, PRs, and code standards.

## Roadmap

- [x] Standalone Desktop App
- [ ] VST3/AU Plugin
- [ ] Server Mode (gRPC)
- [ ] Dedicated Hardware Unit
- [ ] Mobile Remote Control

## License

OpenRig is licensed under the [GNU General Public License v3.0](LICENSE).
