# Audio Backends

## Overview

OpenRig supports four audio backends, each optimized for different use cases. A "backend" is the engine that powers a block model's audio processing. The choice of backend affects sound quality, CPU usage, latency, and parameter flexibility.

## Native (Rust DSP)

Pure Rust digital signal processing. Models are hand-coded algorithms running directly in the audio thread.

**Characteristics:**

- Lowest latency (no external library overhead)
- Lowest CPU usage
- Full parameter control (all parameters adjustable in real time)
- Deterministic behavior

**Used by:** All built-in effects (delay, reverb, modulation, dynamics, filter, wah, tuner), native preamp/amp models (American Clean, Brit Crunch, Chime, etc.), utility blocks.

**Implementation:** Each native model implements the `BlockProcessor` trait directly. The `build()` function in `MODEL_DEFINITION` returns a closure or struct that processes audio sample-by-sample or buffer-by-buffer.

**When to use:** Default choice for new effects. Use Native when you can express the audio algorithm in Rust and need full parameter control.

## NAM (Neural Amp Modeler)

Machine-learning-based amp/pedal modeling. NAM captures the behavior of real hardware by training a neural network on input/output audio pairs.

**Characteristics:**

- Highest realism for amp and pedal tones
- Higher CPU usage (neural network inference per sample)
- Parameters may be limited to capture presets (e.g., gain at 10%, 20%, ..., 100%)
- Requires the NAM C++ engine (built separately)

**Used by:** Most amp models (Marshall, Bogner, Mesa, EVH, Peavey, Dumble), some gain pedals (TS9, BD-2, JHS Andy Timmons), full rig (Roland JC-120B).

**Implementation:** The `nam` crate wraps the NAM C++ engine via FFI. NAM models reference `.nam` capture files (stored in `captures/nam/`). The `block-nam` crate bridges between the NAM engine and the block system.

**Capture files:** Each capture represents the hardware at specific settings. Models with multiple captures (e.g., Marshall JCM 800 at different gain levels) interpolate or switch between captures based on parameter values.

**When to use:** When modeling real hardware (amps, pedals) where realism matters more than CPU efficiency.

## IR (Impulse Response)

Convolution-based processing. An impulse response captures the frequency and phase response of a physical system (speaker cabinet, room, acoustic guitar body).

**Characteristics:**

- Excellent for speaker cabinets and acoustic body simulation
- Fixed response (no adjustable parameters beyond filtering)
- Medium CPU usage (FFT-based convolution)
- Deterministic -- same IR always produces same result

**Used by:** IR cabinet models (Marshall 4x12, Fender Deluxe, Vox AC30 Blue, etc.), all 114 acoustic body models (Taylor, Martin, Gibson, etc.).

**Implementation:** The `ir` crate handles WAV file loading and convolution. IR files are stored in `captures/ir/`. The `block-ir` crate provides the block integration.

**IR files:** Standard WAV format. Typical lengths: 200ms-1s for cabinets, 500ms-2s for rooms/bodies. Stored at the project's sample rate or resampled on load.

**When to use:** For cabinets, rooms, and acoustic body simulation. Use IR when you have a recorded impulse response of the hardware you want to model.

## LV2 (External Plugins)

Open-source audio plugin standard. OpenRig can host LV2 plugins as blocks, expanding the effect library without writing new code.

**Characteristics:**

- Huge library of available effects (275 plugins bundled)
- CPU usage varies per plugin
- Parameters exposed by the plugin (varies)
- External dependency (plugins must be compiled per platform)

**Used by:** Extended gain effects (Chow Centaur, OJD, Wolf Shaper, Bitta, Paranoia, TAP Sigmoid, MDA Overdrive, MDA Degrade).

**Implementation:** The `lv2` crate handles plugin discovery, loading, and hosting. Plugins are stored in `plugins/` directory. Each LV2 plugin is wrapped as a block model with its parameters exposed through the standard `ParameterSet` interface.

**Plugin catalog:** Available plugins are listed in `assets/lv2_catalog.json` with metadata (name, category, ports, parameters).

**When to use:** When an existing LV2 plugin already does what you need. Saves development time but adds external dependency.

## Comparison Table

| | Native | NAM | IR | LV2 |
|---|---|---|---|---|
| **Latency** | Lowest | Low | Low | Low |
| **CPU Usage** | Lowest | High | Medium | Varies |
| **Realism** | Good | Excellent | Excellent | Varies |
| **Parameter Control** | Full | Limited | Minimal | Plugin-dependent |
| **Dependencies** | None | C++ engine | None | Plugin binaries |
| **Best For** | Effects, utilities | Amps, pedals | Cabs, bodies | Extended effects |
| **Development Effort** | High (write DSP) | Medium (train model) | Low (record IR) | Low (wrap plugin) |

## Adding New Captures and IRs

### Adding a NAM capture

1. Train or obtain a `.nam` capture file.
2. Place it in `captures/nam/<brand>/<model>/`.
3. Create a model definition in the appropriate block crate referencing the capture path.
4. The build system auto-registers the new model.

### Adding an IR file

1. Obtain or record a WAV impulse response.
2. Place it in `captures/ir/<category>/<model>/`.
3. Create a model definition in the appropriate block crate.
4. Reference the IR path in the model's `build()` function.

### Adding an LV2 plugin

1. Compile the LV2 plugin for target platforms.
2. Place the plugin bundle in `plugins/`.
3. Update `assets/lv2_catalog.json` with plugin metadata.
4. Create a model definition in the appropriate block crate using the `lv2` crate API.
