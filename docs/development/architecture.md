# Architecture Guide

## Overview

OpenRig follows a clean architecture (hexagonal / ports-and-adapters) pattern. Inner layers define interfaces; outer layers implement them. Dependencies always point inward -- the domain knows nothing about the GUI, database, or audio drivers. This separation ensures that core logic can be tested in isolation and that adapters (GUI, CLI, VST3, gRPC) are interchangeable without touching business rules.

## Architecture Layers

```
┌──────────────────────────────────────────────────────┐
│          Adapters (UI & Infrastructure)               │
│  adapter-gui (Slint), adapter-console, adapter-server │
│  adapter-vst3, infra-yaml, infra-filesystem, infra-cpal │
├──────────────────────────────────────────────────────┤
│                 Engine (Runtime)                       │
│  Audio graph, real-time DSP processing, block chain   │
├──────────────────────────────────────────────────────┤
│              Application (Orchestration)               │
│  Commands, validation, use cases                      │
├──────────────────────────────────────────────────────┤
│                Domain (Core Models)                    │
│  Blocks, chains, parameters, value objects            │
└──────────────────────────────────────────────────────┘
```

Each layer has a well-defined responsibility:

- **Domain** -- Pure data structures and business rules. No I/O, no frameworks.
- **Application** -- Orchestrates domain operations, enforces validation, exposes use cases.
- **Engine** -- Manages the real-time audio graph, scheduling block processing within buffer deadlines.
- **Adapters** -- Translate between external systems (GUI, CLI, audio hardware, file formats) and the application layer.

## Crate Map

The project is organized into 38 crates across five logical groups.

### Core Domain and Application (5 crates)

| Crate | Purpose |
|-------|---------|
| `domain` | Domain models, IDs, value objects |
| `application` | Orchestration, validation, commands |
| `engine` | Runtime audio graph, real-time DSP processing |
| `project` | Project structure, chains, device settings |
| `state` | Application state management |

### Audio Processing Blocks (17 crates)

| Crate | Purpose |
|-------|---------|
| `block-core` | Base types: `BlockProcessor`, `AudioChannelLayout`, `ParameterSet` |
| `block-preamp` | Preamp models (5 models: 3 Native, 2 NAM) |
| `block-amp` | Amplifier models (12 models: 3 Native, 9 NAM) |
| `block-cab` | Cabinet models (11 models: 3 Native, 8 IR) |
| `block-gain` | Gain/overdrive/distortion (13 models: Native, NAM, LV2) |
| `block-delay` | Delay effects (6 Native models) |
| `block-reverb` | Reverb effects (1 Native model) |
| `block-mod` | Modulation effects (5 Native models) |
| `block-dyn` | Dynamics -- compressor, gate (2 Native models) |
| `block-filter` | EQ and filters (1 Native model) |
| `block-wah` | Wah pedal (1 Native model) |
| `block-pitch` | Pitch shifting effects |
| `block-util` | Utility blocks -- tuner (1 Native model) |
| `block-body` | Acoustic body resonance (114 IR models) |
| `block-full-rig` | All-in-one rigs (1 NAM model) |
| `block-routing` | Signal routing utilities |
| `block-thumbnails` | Block thumbnail generation |

### Audio Backend Integration (4 crates)

| Crate | Purpose |
|-------|---------|
| `nam` | Neural Amp Modeler C++ engine integration |
| `ir` | Impulse Response convolution engine |
| `lv2` | LV2 plugin host integration |
| `asset-runtime` | `EmbeddedAsset`, `materialize()` for compiled-in assets |

### Adapters and UI (6 crates)

| Crate | Purpose |
|-------|---------|
| `adapter-gui` | Desktop GUI (Slint framework) |
| `adapter-console` | CLI interface |
| `adapter-server` | gRPC server mode |
| `adapter-vst3` | VST3/AU plugin wrapper |
| `ui-openrig` | Shared UI components |
| `block-nam` | NAM block integration (bridge between `nam` engine and `block-core`) |

### Infrastructure (5 crates)

| Crate | Purpose |
|-------|---------|
| `infra-yaml` | YAML serialization/deserialization |
| `infra-filesystem` | File I/O adapters |
| `infra-cpal` | Audio device integration via CPAL library |
| `preset` | Preset file handling |
| `ports` | Port/interface definitions |

> **Note:** `block-ir` also serves as infrastructure for IR file loading.

## Audio Signal Flow

The audio pipeline processes samples through a sequential block chain:

1. **CPAL captures audio** from the system input device and fills a buffer.
2. **Engine receives the audio buffer** on the real-time audio thread.
3. **Buffer passes through the block chain** sequentially:

```
Input Block --> [Effect Blocks...] --> Output Block
```

4. **Each block's `process()` method** transforms the audio data in place.
5. **CPAL sends the processed buffer** to the system output device.

The chain runs on a dedicated real-time audio thread. Every block must complete its processing within the buffer deadline. For example, at 48 kHz sample rate with a 256-sample buffer, each block chain invocation must finish within approximately 5.3 ms to avoid audio dropouts.

## Key Design Patterns

### Registry Pattern (build.rs auto-discovery)

Each block crate contains a `build.rs` script that scans source files for `MODEL_DEFINITION` constants at compile time. It generates a `generated_registry.rs` file containing an array of all discovered model definitions. This means adding a new model to the system requires only creating a new `.rs` file with a `pub const MODEL_DEFINITION` -- no manual registration, no central manifest to update.

### Adapter Pattern

All four adapters (GUI, console, server, VST3) share the same application layer. Each adapter is responsible only for I/O presentation and user interaction. Business logic, validation, and orchestration live exclusively in the domain and application layers. Swapping or adding a new adapter has zero impact on core behavior.

### Asset Pipeline

Assets such as SVG images, NAM neural network captures, and IR wav files are embedded into the binary at compile time via `EmbeddedAsset`. At runtime, the `materialize()` function extracts these assets to disk on demand. This is necessary because external engines (NAM, LV2) require file paths rather than in-memory buffers. The approach guarantees that the application is fully self-contained with no external asset dependencies.

### Zero Coupling Rule

Block implementations never reference specific models, brands, or effect types directly. Every block implements the same `BlockProcessor` trait. The registry provides metadata (name, category, parameter definitions); the engine treats all blocks uniformly. This design allows new models to be added without modifying any existing code outside the new model file itself.
