<p align="center">
  <img src="crates/adapter-gui/ui/assets/openrig-logomark.svg" alt="OpenRig logomark" height="120"><img src="crates/adapter-gui/ui/assets/openrig-logotype.png" alt="OpenRig" height="120">
</p>

<p align="center">
  <strong>Build your rig once. Use it everywhere.</strong>
</p>

<p align="center">
  <a href="LICENSE"><img src="https://img.shields.io/badge/license-GPL--3.0-blue.svg" alt="License: GPL-3.0"></a>
  <img src="https://img.shields.io/badge/version-0.1.0-orange.svg" alt="Version: 0.1.0">
  <img src="https://img.shields.io/badge/rust-2021_edition-brightgreen.svg" alt="Rust: 2021 Edition">
  <img src="https://img.shields.io/badge/platforms-macOS%20%7C%20Linux%20%7C%20Windows-lightgrey.svg" alt="Platforms: macOS | Linux | Windows">
  <a href="https://github.com/jpfaria/OpenRig/actions/workflows/test.yml"><img src="https://github.com/jpfaria/OpenRig/actions/workflows/test.yml/badge.svg?branch=develop" alt="Tests"></a>
  <a href="https://codecov.io/gh/jpfaria/OpenRig"><img src="https://codecov.io/gh/jpfaria/OpenRig/branch/develop/graph/badge.svg" alt="Coverage"></a>
</p>

<p align="center">
  <img src="docs/assets/sc1.png" alt="OpenRig Project view — multiple parallel chains with amp, pedal, and cab blocks" width="900">
</p>

---

## What is OpenRig?

OpenRig is a virtual pedalboard and rig platform for guitarists, bassists, and any musician who wants serious tone in software. You build signal chains visually — input, pedals, amp, cab, output — and OpenRig processes audio in real time with professional-grade quality, low latency, and zero compromises on stream isolation between parallel inputs.

Built in Rust for performance and reliability, OpenRig combines four audio backends — **Native DSP**, **Neural Amp Modeler (NAM)**, **Impulse Response (IR)** convolution, and **LV2 plugins** — into a single platform that runs on macOS, Linux, and Windows. Every block in the chain can come from any backend, and they all coexist in the same real-time audio graph.

The long-term vision: one rig that works as a standalone app, a VST3 plugin in your DAW, a server for remote setups, and dedicated hardware on stage.

## Showcase

<p align="center">
  <img src="docs/assets/sc2.png" alt="Block library — vertical list of pedals and amps with brand-accurate panel art" width="280">&nbsp;&nbsp;&nbsp;
  <img src="docs/assets/sc3.png" alt="Block editor — Marshall JTM45 panel with channel and volume knobs" width="600">
</p>

<p align="center">
  <em>Left: block library with brand-organized panel art. Right: per-block editor with hardware-faithful controls.</em>
</p>

## Why OpenRig?

- **Real captures of real hardware.** NAM models are nonlinear neural captures of physical amps and pedals — Marshall Plexi, Mesa Rectifier, EVH 5150, Vox AC30, Klon Centaur, Boss DS-1, Big Muff, and 540+ more. Not parametric approximations.
- **One platform, four backends.** Native zero-latency DSP for utility blocks, NAM for nonlinear gear, IR convolution for cabs and acoustic body resonance, and 100+ LV2 plugins (Guitarix, MDA, TAP, ZAM, Dragonfly, and others) — all in the same graph, with no glue tax.
- **Truly parallel chains.** Each input is an isolated audio runtime — no shared buffers, no contended locks, no cross-stream CPU spikes. Two guitars on the same interface? Two completely independent rigs.
- **Real-time visualization built in.** Spectrum analyzer and chromatic tuner are first-class blocks you drop into the chain like any other. See what you hear.
- **Open preset format.** Presets are plain YAML — diffable, gist-shareable, scriptable. Build a preset by hand or let the [`openrig-tone-builder`](.claude/skills/openrig-tone-builder/SKILL.md) Claude Code skill recreate the tone of a specific song from public sources.
- **Cross-platform without compromises.** macOS Core Audio, Linux ALSA + JACK, Windows WASAPI — each with platform-isolated paths. Linux even runs on Orange Pi (aarch64) for dedicated hardware deployments.

## By the Numbers

| | |
|---|---|
| **560+** registered models | across 16 block types |
| **4** audio backends | Native DSP · NAM · IR · LV2 |
| **5** platform targets | macOS (aarch64+x86_64), Linux (x86_64+aarch64), Windows (x86_64) |
| **38** Rust crates | clean/hexagonal architecture |
| **114** acoustic body IRs | Martin · Taylor · Gibson · Yamaha · Takamine · Guild · Ibanez · and more |
| **100+** LV2 plugins | bundled, including 33 from the Guitarix project |

## Block Types

| Type | Models | Backends | What it covers |
|------|-------:|----------|----------------|
| **Preamp**     |  39 | Native, NAM        | Pre-amplification stage with gain and EQ voicing |
| **Amp**        | 141 | Native, NAM, LV2   | Full-rig captures of vintage Marshalls, Mesa, Fender, Vox, EVH, Friedman, Diezel, Bogner, Soldano, Orange, and others |
| **Cab**        |  29 | Native, IR, LV2    | Speaker cabinet IRs — V30 4×12, Greenback, AC30 Blue, Mesa OS, Fender Oxford, multi-mic options |
| **Gain**       | 154 | Native, NAM, LV2   | Overdrive, distortion, fuzz, boost — Klon Centaur, ProCo RAT, Big Muff, DS-1/DS-2, OCD, BD-2, Tube Screamer, HM-2, plus 33 Guitarix LV2 drives |
| **Delay**      |  15 | Native, LV2        | Digital, analog, slapback, reverse, modulated, tape vintage |
| **Reverb**     |  20 | Native, LV2        | Plate, hall, room, spring, plus Dragonfly, MVerb, MDA Ambience, CAPS Plate |
| **Modulation** |  17 | Native, LV2        | Chorus, ensemble chorus, tremolo, vibrato, phaser, flanger, ring mod, Leslie |
| **Dynamics**   |  10 | Native, LV2        | Studio-clean compressor, gate, brickwall limiter, de-esser, multiband |
| **Filter**     |  14 | Native, LV2        | 3-band EQ, guitar EQ, 8-band parametric EQ, auto-filter, auto-wah, HP/LP |
| **Wah**        |   2 | Native, LV2        | Cry Classic, GxQuack |
| **Pitch**      |   4 | LV2                | Harmonizer, x42 chromatic autotune, MDA Detune, RePsycho |
| **Body**       | 114 | IR                 | Acoustic body resonance for piezo/magnetic pickups |
| **Utility**    |   2 | Native             | Chromatic tuner, real-time spectrum analyzer |
| **IR Loader**  |   1 | Native             | Load any user-supplied WAV impulse response |
| **NAM Loader** |   1 | NAM                | Load any user-supplied `.nam` capture |
| **I/O**        |   — | platform           | Multi-input/multi-output routing with per-stream isolation |

See the complete [Blocks Reference](docs/user-guide/blocks-reference.md) for every model with canonical IDs, parameters, and ranges.

## Audio Backends

OpenRig is multi-backend by design — each backend earns its place because it does something the others can't.

| Backend | What it gives you |
|---------|-------------------|
| **Native DSP**     | Pure Rust signal processing for utility, EQ, dynamics, modulation, and reverb blocks. Lowest latency, lowest CPU, fully real-time-controllable parameters. |
| **NAM** (Neural Amp Modeler) | Capture-based modeling of nonlinear hardware. Each NAM model is a neural network trained on a real amp or pedal at fixed knob settings — the most accurate way to reproduce a specific tone in software. Higher CPU than Native, but indispensable for amp and drive blocks. |
| **IR** (Impulse Response) | Convolution-based simulation for speaker cabinets and acoustic guitar body resonance. A single IR captures the full frequency response of a mic'd cab or a guitar's air. |
| **LV2**            | Open-source plugin standard. OpenRig bundles 100+ LV2 plugins from Guitarix, MDA, TAP, ZAM, Dragonfly, CAPS, FOMP, and others — drives, cabs, reverbs, modulation, autotune, and more. |

## Quick Start

1. **Launch OpenRig** and create a new project.
2. **Configure I/O** — pick your audio interface as input (your guitar) and output (headphones or monitors).
3. **Build a chain** visually — drag blocks between Input and Output: Tuner → EQ → Drive → Amp → Cab → Reverb.
4. **Tweak in real time** — click any block to open its editor and turn knobs while you play.
5. **Save it as a preset** — presets are plain YAML in `~/.openrig/presets/` (macOS/Linux) or `%APPDATA%\OpenRig\presets\` (Windows). Share by copy-paste.

See the [Quick Start Guide](docs/user-guide/quick-start.md) for a complete walkthrough.

## Build Your Tone

Presets in OpenRig are plain YAML. Here's the start of a Frusciante-style chain for "Can't Stop":

```yaml
id: red_hot_chili_peppers_-_cant_stop_-_rhythm
name: Red Hot Chili Peppers - Can't Stop (Rhythm)
blocks:
  - type: gain
    enabled: true
    model: cc_boost            # MXR Micro Amp clean boost
    params: {}
  - type: gain
    enabled: true
    model: boss_ds1            # Boss DS-2 proxy: tone 7, dist 5
    params: { tone: 7, dist: 5 }
  - type: modulation
    enabled: true
    model: ensemble_chorus     # CE-1 Chorus Ensemble
    params: { rate_hz: 0.55, depth: 22.0, mix: 25.0 }
  - type: amp
    enabled: true
    model: marshall_super_100_1966   # Marshall Major proxy
    params: {}
  # ...post-amp EQ, reverb, limiter, master volume
```

Every `model:` ID is registered in the [Blocks Reference Quick Reference](docs/user-guide/blocks-reference.md#model-id-quick-reference). For the Claude Code crowd, the [`openrig-tone-builder`](.claude/skills/openrig-tone-builder/SKILL.md) skill builds full presets from a song name — researches the original signal chain from public sources, maps real gear to OpenRig models, and writes the YAML.

## Architecture

```
┌──────────────────────────────────────────────────────────┐
│   Adapters — GUI (Slint), Console, Server (gRPC), VST3   │
├──────────────────────────────────────────────────────────┤
│   Engine — real-time audio graph, block chain processing │
├──────────────────────────────────────────────────────────┤
│   Application — orchestration, validation, commands      │
├──────────────────────────────────────────────────────────┤
│   Domain — blocks, chains, parameters, value objects     │
└──────────────────────────────────────────────────────────┘
```

OpenRig is organized as a Rust workspace with **38 crates** spanning core domain, audio processing blocks, backend integration, adapters/UI, and infrastructure. The architecture follows a clean/hexagonal pattern — inner layers define interfaces, outer layers implement them. Audio processing is split per block type (`block-amp`, `block-gain`, `block-cab`, `block-reverb`, etc.) so a model is fully owned by its crate, with zero cross-coupling between brand-specific captures and the rest of the system.

See [Architecture](docs/development/architecture.md) for the full crate map, audio signal flow, and design patterns.

## Installation

### Download

| Platform | Architecture | Download |
|----------|-------------|----------|
| macOS    | Apple Silicon (aarch64) | [Latest Release](https://github.com/jpfaria/OpenRig/releases/latest) |
| macOS    | Intel (x86_64)          | [Latest Release](https://github.com/jpfaria/OpenRig/releases/latest) |
| Linux    | x86_64                  | [Latest Release](https://github.com/jpfaria/OpenRig/releases/latest) |
| Linux    | aarch64 (incl. Orange Pi) | [Latest Release](https://github.com/jpfaria/OpenRig/releases/latest) |
| Windows  | x86_64                  | [Latest Release](https://github.com/jpfaria/OpenRig/releases/latest) |

### Build from Source

```bash
git clone https://github.com/jpfaria/OpenRig.git
cd OpenRig
git submodule update --init --recursive
cargo build --release -p adapter-gui
```

See the [Installation Guide](docs/user-guide/installation.md) for detailed instructions, platform-specific dependencies, and troubleshooting.

## Documentation

### For Musicians

- [Installation Guide](docs/user-guide/installation.md) — download, build, and set up OpenRig
- [Quick Start](docs/user-guide/quick-start.md) — create your first project and signal chain
- [Blocks Reference](docs/user-guide/blocks-reference.md) — every model with canonical IDs and parameters
- [Presets](docs/user-guide/presets.md) — create, save, and share chain configurations

### For Developers

- [Architecture](docs/development/architecture.md) — crate map, layers, and design patterns
- [Building](docs/development/building.md) — full build guide including the NAM engine and Docker
- [Creating Blocks](docs/development/creating-blocks.md) — how to add new audio models
- [Audio Backends](docs/development/audio-backends.md) — Native, NAM, IR, and LV2 internals

## Contributing

OpenRig welcomes contributions. We follow [Gitflow](https://nvie.com/posts/a-successful-git-branching-model/) with strict code quality standards — zero warnings, zero coupling, single source of truth.

See [CONTRIBUTING.md](CONTRIBUTING.md) for the complete guide on branching, commits, PRs, and code standards.

## Roadmap

- [x] Standalone Desktop App (macOS, Linux, Windows)
- [x] Multi-input parallel chains with stream isolation
- [x] 560+ models across 16 block types
- [ ] VST3/AU Plugin
- [ ] Server Mode (gRPC) — remote rigs over network
- [ ] Dedicated Hardware Unit (Orange Pi-based, low-latency Linux)
- [ ] Mobile Remote Control

## License

OpenRig is licensed under the [GNU General Public License v3.0](LICENSE).
