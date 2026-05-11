# Documentation Overhaul — Design Spec

**Issue:** #84
**Date:** 2026-03-29
**Status:** Approved

## Goal

Create professional, comprehensive documentation for OpenRig that serves two audiences: musicians/end users and developers/contributors. The documentation follows a Hub & Spoke model where the README acts as a landing page with links to detailed docs.

## Decisions

| Decision | Choice | Rationale |
|----------|--------|-----------|
| Audience | Both (musicians + devs), separated paths | OpenRig is both a product and an open-source project |
| License | GPL v3 | Protects against proprietary forks, consistent with audio OSS ecosystem |
| Language | English | Global project, Rust ecosystem is international |
| Screenshots | None for now | UI is evolving (v0.1.0), will add when stable |
| Structure | Hub & Spoke | Project too large for monolith README (38 crates, 170 models, 5 platforms) |
| Getting Started | Binaries + build from source | Both paths needed |
| Platforms | 5 (macOS aarch64/x86_64, Linux aarch64/x86_64, Windows x86_64) | All supported |
| CHANGELOG | Out of scope | Separate issue |

## File Structure

```
README.md                              ← Hub (landing page, ~150-200 lines)
LICENSE                                ← GPL v3
docs/
  user-guide/
    installation.md                    ← Binaries + build from source
    quick-start.md                     ← First rig tutorial
    blocks-reference.md                ← 14 types, 170 models, all parameters
    presets.md                         ← Create/save/load presets
  development/
    architecture.md                    ← 38 crates, clean architecture layers
    building.md                        ← Full build (NAM, Docker, CI)
    creating-blocks.md                 ← How to create new blocks/models
    audio-backends.md                  ← Native, NAM, IR, LV2
```

## README.md — Detailed Design

### 1. Header

```
[OpenRig Logo — centered, from docs/assets/openrig-logo.svg]

# OpenRig

**Build your rig once. Use it everywhere.**

[badges: GPL-3.0 license | platforms | build status | version 0.1.0]
```

- Logo: use existing `docs/assets/openrig-logo.svg`, reference via GitHub raw URL
- Badges: shields.io, standard GitHub badge format
- Tagline already exists in current README, keep it

### 2. What is OpenRig?

Two paragraphs:
1. **Product pitch:** Virtual pedalboard/rig platform for musicians. Build signal chains with amp models, effects, cabs. Real-time audio processing with professional-grade quality.
2. **Technical pitch:** Built in Rust for performance. 4 audio backends (Native DSP, Neural Amp Modeler, Impulse Response, LV2 plugins). Runs on 5 platforms. Vision: standalone → VST3 → server → hardware.

### 3. Features

Compact list, no emojis (professional tone):

- **170+ amp and effect models** — preamps, amps, cabs, gain pedals, delays, reverbs, modulation, dynamics, filters, wah, tuner, body resonance, full rigs
- **4 audio backends** — Native Rust DSP, Neural Amp Modeler (NAM), Impulse Response (IR), LV2 plugins
- **Chain-based signal routing** — drag-and-drop blocks, per-chain instrument filtering
- **Real-time parameter control** — knobs, sliders, switches with instant audio feedback
- **5 platforms** — macOS (Apple Silicon + Intel), Linux (x86_64 + ARM), Windows
- **Multiple instruments** — electric guitar, acoustic guitar, bass, voice, keys, drums

### 4. Block Types

Summary table:

| Type | Models | Backends | Description |
|------|--------|----------|-------------|
| Preamp | 4 | Native, NAM | Pre-amplification, gain, EQ |
| Amp | 12 | Native, NAM | Full amplifier (preamp + power amp + cab) |
| Cab | 9 | Native, IR | Speaker cabinet simulation |
| ... | ... | ... | ... |

Link: "See the complete [Blocks Reference](docs/user-guide/blocks-reference.md) for all 170 models and parameters."

### 5. Installation

Platform table:

| Platform | Architecture | Download |
|----------|-------------|----------|
| macOS | Apple Silicon (aarch64) | [Download](link) |
| macOS | Intel (x86_64) | [Download](link) |
| Linux | x86_64 | [Download](link) |
| Linux | aarch64 | [Download](link) |
| Windows | x86_64 | [Download](link) |

Brief build from source (3 commands):
```bash
git clone https://github.com/jpfaria/OpenRig.git
cd OpenRig
cargo build --release -p adapter-gui
```

Link: "See [Installation Guide](docs/user-guide/installation.md) for detailed instructions and troubleshooting."

### 6. Quick Start

5 steps, one line each:
1. Launch OpenRig
2. Create a new project
3. Configure audio devices (input/output)
4. Add blocks to your chain (preamp → cab → delay → reverb)
5. Play and adjust parameters in real time

Link: "See the [Quick Start Guide](docs/user-guide/quick-start.md) for a complete walkthrough."

### 7. Architecture

Text-based layer diagram:

```
┌─────────────────────────────────────────┐
│  Adapters (GUI, Console, Server, VST3)  │
├─────────────────────────────────────────┤
│  Engine (audio graph, real-time DSP)    │
├─────────────────────────────────────────┤
│  Application (orchestration, commands)  │
├─────────────────────────────────────────┤
│  Domain (models, blocks, chains)        │
└─────────────────────────────────────────┘
```

Brief mention of 38 crates in 5 groups.

Link: "See [Architecture](docs/development/architecture.md) for the full crate map and design patterns."

### 8. Contributing

Short paragraph: "OpenRig welcomes contributions. We follow Gitflow with strict code quality standards (zero warnings, zero coupling). See [CONTRIBUTING.md](CONTRIBUTING.md) for the complete guide."

### 9. Roadmap

```
- [x] Standalone Desktop App
- [ ] VST3/AU Plugin
- [ ] Server Mode (gRPC)
- [ ] Dedicated Hardware Unit
- [ ] Mobile Remote Control (Flutter)
```

### 10. License

"OpenRig is licensed under the [GNU General Public License v3.0](LICENSE)."

## User Guide — Detailed Design

### installation.md

**Sections:**
1. **System Requirements** — minimum OS versions, disk space, audio interface recommended
2. **Download Binaries** — table identical to README but with SHA256 checksums
3. **Build from Source:**
   - Install Rust (rustup)
   - System dependencies per platform:
     - macOS: `brew install cmake pkg-config`
     - Linux (Ubuntu/Debian): `apt install cmake pkg-config libasound2-dev`
     - Linux (Fedora): `dnf install cmake pkg-config alsa-lib-devel`
     - Windows: Visual Studio Build Tools, cmake
   - Clone and build: `cargo build --release -p adapter-gui`
   - NAM engine build (optional, for NAM models): cmake steps
   - Prebuilt libs: where to download, how to place
4. **Troubleshooting** — common errors (ALSA not found, cmake version, linker errors)

### quick-start.md

**Sections:**
1. **Concepts** — project (workspace), chain (signal path), block (processor), model (specific implementation), parameter (adjustable value), instrument (filters available blocks)
2. **Create Your First Project** — step by step with expected UI descriptions
3. **Configure Audio** — selecting input/output devices, sample rate, buffer size considerations
4. **Build a Chain** — adding blocks: Input → Preamp → Cab → Delay → Reverb → Output
5. **Adjust Parameters** — how to tweak knobs/sliders, what each parameter does
6. **Save and Load** — project persistence, YAML format mention

### blocks-reference.md

**Structure per block type:**
```markdown
## Preamp

Pre-amplification stage with gain and EQ controls.

| Model | Brand | Backend | Description |
|-------|-------|---------|-------------|
| American Clean | — | Native | Clean American-style preamp |
| Brit Crunch | — | Native | British crunch preamp |
| Modern High Gain | — | Native | Modern high-gain preamp |
| Marshall JCM 800 2203 | Marshall | NAM | Classic British crunch/gain |

### Parameters (Native models)

| Parameter | Range | Default | Description |
|-----------|-------|---------|-------------|
| gain | 0–100% | 50% | Input gain |
| bass | 0–100% | 50% | Low frequency EQ |
| ... | ... | ... | ... |
```

Repeat for all 14 block types. Source data from CLAUDE.md and Rust registry code.

### presets.md

**Sections:**
1. **What is a Preset** — a saved chain configuration
2. **YAML Format** — annotated example of a complete chain YAML
3. **Example Chains:**
   - Clean: Input → American Clean preamp → American 2x12 cab → Plate reverb → Output
   - Crunch: Input → Brit Crunch preamp → Brit 4x12 cab → Analog delay → Output
   - High Gain: Input → TS9 gain → Modern High Gain preamp → Brit 4x12 cab → Output
   - Acoustic: Input → Body resonance → Three Band EQ → Plate reverb → Output
4. **Sharing Presets** — copy YAML files, community conventions

## Development Guide — Detailed Design

### architecture.md

**Sections:**
1. **Overview** — clean architecture, dependency rule (inner layers don't know outer layers)
2. **Layer Diagram** — expanded version of README diagram with crate names
3. **Crate Map** — all 38 crates organized in 5 groups:
   - Core Domain & App (4): domain, project, application, engine
   - Infrastructure (3): infra-yaml, infra-filesystem, infra-cpal
   - Audio Processing Blocks (25): block-core, block-preamp, block-amp, block-cab, etc.
   - UI/Adapters (5): ui-openrig, adapter-gui, adapter-console, adapter-server, adapter-vst3
   - Utilities (3+): preset, state, ports
4. **Audio Signal Flow** — how audio travels: CPAL input → engine graph → block chain → CPAL output
5. **Key Patterns:**
   - Registry pattern (build.rs auto-discovery of MODEL_DEFINITION)
   - Adapter pattern (gui/console/server/vst3 all use same application layer)
   - Asset pipeline (EmbeddedAsset, materialize, controls.svg)

### building.md

**Sections:**
1. **Prerequisites** — Rust toolchain version, system tools
2. **Quick Build** — `cargo build -p adapter-gui` for just the GUI
3. **Full Build** — all adapters, NAM engine compilation, IR processing
4. **NAM Engine** — C++/CMake build, prebuilt libraries per platform, Git LFS for models
5. **Docker** — cross-compilation setup for all 5 platforms
6. **CI/CD** — GitHub Actions workflow overview, build-libs.yml, claude.yml
7. **Build Targets:**
   - `adapter-gui` — desktop GUI (Slint)
   - `adapter-console` — CLI interface
   - `adapter-server` — gRPC server
   - `adapter-vst3` — VST3 plugin

### creating-blocks.md

**Sections:**
1. **Overview** — what a block is, the MODEL_DEFINITION pattern
2. **Step by Step:**
   1. Create a new `.rs` file in the appropriate `block-*` crate
   2. Define `pub const MODEL_DEFINITION: XxxModelDefinition`
   3. Implement schema, validate, asset_summary, build functions
   4. Build — `build.rs` auto-discovers and registers
3. **Naming Conventions** — prefixes (native_, nam_, ir_), display names, brand field
4. **Assets:**
   - `controls.svg` — panel image following AC30 reference pattern
   - `component.yaml` — asset paths and SVG control positions
   - Brand logos — worldvectorlogo.com, fill="currentColor"
5. **Testing** — how to test a new block processes audio correctly

### audio-backends.md

**Sections:**
1. **Overview** — why 4 backends, when to use each
2. **Native** — pure Rust DSP, lowest latency, lowest CPU, parameters fully controllable
3. **NAM (Neural Amp Modeler)** — C++ engine, capture-based, most realistic for amps/pedals, higher CPU
4. **IR (Impulse Response)** — convolution engine, used for cabs and acoustic bodies, fixed response
5. **LV2** — external open-source plugins, 275 available, loaded dynamically
6. **Comparison Table:**

| Backend | Latency | CPU | Realism | Flexibility | Use Case |
|---------|---------|-----|---------|-------------|----------|
| Native | Lowest | Lowest | Good | Full control | Built-in effects |
| NAM | Low | High | Excellent | Capture-based | Amp/pedal modeling |
| IR | Low | Medium | Excellent | Fixed | Cabs, acoustic bodies |
| LV2 | Low | Varies | Varies | Plugin-dependent | Extended effects |

7. **Adding Captures/IRs** — file formats, directory structure, registration

## Existing Docs — Reorganization

### Keep as-is:
- `docs/adr/` — Architecture Decision Records (may add new ones)
- `docs/gui/` — GUI design specs (internal reference)
- `docs/superpowers/` — AI agent plans/specs (internal)

### Consolidate:
- `docs/backend/current-contract.md` → reference from `docs/development/audio-backends.md`
- `docs/backend/native-model-catalog.md` → reference from `docs/user-guide/blocks-reference.md`

No content is deleted — existing docs are referenced from the new structure, not duplicated.

## Out of Scope

- CHANGELOG.md — separate issue
- Screenshots/GIFs — UI still evolving
- mdBook/docs website — future upgrade
- API documentation (rustdoc) — future
- Translations (pt-BR) — future
- Video tutorials — future
