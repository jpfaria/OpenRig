# Audio Backends

A backend is the engine that runs a block model. OpenRig has five. Native models are compiled into OpenRig; the other four load **plugin packages** from [OpenRig-plugins](https://github.com/jpfaria/OpenRig-plugins) at runtime through `crates/plugin-loader`, which walks the plugins folder, reads each package's `manifest.yaml` and registers its models in the catalog.

Per-backend parameters and engine details are in the [blocks catalog](../blocks-catalog.md).

## Native (Rust DSP)

Hand-written DSP in the block crates (`block-delay`, `block-reverb`, …), running directly on the audio thread.

- Lowest latency and CPU cost; every parameter adjustable in real time; deterministic.
- Used by every built-in model: native preamps, amps and cabs, gain pedals, delays, reverbs, modulation, dynamics, filters, wahs and the pitch shifter.
- Each model is a file with a `MODEL_DEFINITION` whose `build()` returns a `BlockProcessor`. To write one, see [Creating Blocks](creating-blocks.md).
- The default for a new effect you can express as an algorithm.

## NAM (Neural Amp Modeler)

Neural captures of real amps, preamps and pedals.

- The most realistic amp and pedal tones; higher CPU (network inference per sample).
- A model's knobs pick among its captures (e.g. gain at fixed steps): the nearest capture on the grid is loaded.
- `crates/nam` wraps the vendored NeuralAmpModelerCore (C++) through the `cpp/nam_wrapper` FFI, built by cargo. `crates/block-nam` adds the generic `neural_amp_modeler` loader for any `.nam` file.
- A1 (WaveNet / LSTM / ConvNet) and A2 (`SlimmableContainer`) captures are both supported.

## IR (Impulse Response)

Convolution with a recorded impulse response: speaker cabinets, acoustic bodies and reverbs.

- Fixed response; uniformly partitioned FFT convolution (`crates/ir`), even per-callback cost, 64 samples of added latency.
- A cab or body IR is 100 % wet; a reverb IR blends dry/wet with `mix`.
- IR files are standard WAV and are resampled on load to the running rate (`crates/ir/src/ir_prepare.rs`). `crates/block-ir` adds the generic `generic_ir` loader for any `.wav` file.

## LV2

Open-source plugins hosted in-process by `crates/lv2`.

- Extends the effect library without writing DSP; CPU and parameters depend on the plugin.
- Plugins must be compiled per platform: a package ships one binary per platform, and a package with no binary for the current platform is skipped. The binaries are built with `scripts/build-lib.sh` — see [Building](building.md#lv2-plugin-libraries).

## VST3

VST3 plugins hosted by `crates/vst3-host`, with the plugin's own editor window.

- Found in the standard OS VST3 folders and in the `vst3/` folder of the plugins root, so a user-installed VST3 and one shipped in OpenRig-plugins land in the same list.
- Parameters are read at runtime from the plugin's controller, not from a manifest.

## Comparison

| | Native | NAM | IR | LV2 | VST3 |
|---|---|---|---|---|---|
| **Latency** | Lowest | Low | 64 samples | Plugin-dependent | Plugin-reported |
| **CPU** | Lowest | High | Medium | Varies | Varies |
| **Parameter control** | Full | Capture grid + engine knobs | Minimal | Plugin-dependent | Plugin-dependent |
| **Lives in** | This repository | OpenRig-plugins | OpenRig-plugins | OpenRig-plugins | OpenRig-plugins or the OS |
| **Best for** | Effects | Amps, pedals | Cabs, bodies, reverbs | Extended effects | Commercial plugins |

## Adding a model

- **Native** — a Rust file in a block crate: [Creating Blocks](creating-blocks.md).
- **NAM, IR, LV2, VST3** — a package in OpenRig-plugins: a folder `<backend>/<id>/` with a `manifest.yaml` and its files. Follow that repository's docs; nothing changes in this one.
