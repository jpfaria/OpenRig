# OpenRig

![OpenRig logo](docs/assets/openrig-logo.svg)

OpenRig is a cross-platform pedalboard platform for guitar and acoustic processing, built around one Rust DSP core that can run as a standalone app, a VST3 plugin, a server process, and a dedicated hardware unit.

It is being designed to run on Windows, macOS, and Linux, and to scale from desktop audio to a dedicated hardware rig with remote control interfaces.

## Vision

- One pedalboard core with reusable DSP, routing, setup, and state handling
- Standalone desktop application for direct use on Windows, macOS, and Linux
- VST3 plugin with a native GUI for integration inside DAWs
- Server mode for headless audio hosting and remote control
- Flutter client applications for desktop and mobile control surfaces
- Dedicated hardware version with a custom enclosure and integrated controls

## Direction

- Core-first design
- Clear separation between setup, application, engine, infrastructure, and adapters
- Generic audio blocks instead of a NAM-only model
- NAM, IR, internal effects, control surfaces, and runtime hosts treated as distinct concerns
- First priority is a simple working path end to end

## Product Modes

- `Standalone`: OpenRig owns the audio device and runs as the main pedalboard application
- `VST3`: OpenRig runs inside a DAW with the same processing model and a dedicated plugin GUI
- `Server`: OpenRig runs headless and exposes control endpoints for remote UIs
- `Hardware`: OpenRig runs on a dedicated unit with an embedded screen and foot control workflow
- `Remote UI`: Flutter clients can control rigs, presets, and state from desktop or mobile

## Hardware Vision

The hardware path is designed around a shared stage unit that can receive multiple instruments and let each musician control their own track from a phone or tablet.

See [docs/hardware.md](docs/hardware.md) for the product model and responsibilities of the hardware host versus the remote clients.
See [docs/image-prompts.md](docs/image-prompts.md) for production prompts to generate realistic marketing images of the hardware, guitars, and mobile control workflow.

## Current Vertical Slice

The current implementation is focused on a minimal working path:

- Load setup from YAML
- Validate the setup structure
- Resolve audio devices with CPAL
- Load NAM processors through the native wrapper
- Apply IR inside the NAM plugin-style processing path
- Build a simple per-track runtime queue
- Run audio input and output streams from the console adapter

## Stage Catalog
Implemented today:
- `stage-amp-nam`: NAM amp/pedal model processing with plugin-style controls and IR support
- `stage-delay-digital`: digital delay
- `stage-reverb-plate`: plate reverb foundation
- `stage-util-tuner`: chromatic tuner
- `stage-dyn-compressor`: compressor
- `stage-dyn-gate`: noise gate
- `stage-filter-eq`: three-band EQ
- `stage-mod-tremolo`: tremolo
Planned next expansions:
- `stage-delay-*`: tape, analog, dual, ping-pong
- `stage-reverb-*`: spring, hall, room as distinct algorithms
- `stage-mod-*`: chorus, phaser, flanger, rotary
- `stage-gain`: boost, overdrive, distortion, fuzz
- `stage-pitch`: octave, harmonizer, detune
- `stage-cab`: IR and cabinet stage wiring in the live chain
- `stage-amp`: preamp, amp, and power amp stages
Sources and inspirations:
- Native OpenRig implementations live in the `stage-*` crates
- Several first-pass DSP ideas are ported or adapted from the local `rustortion` reference project
- Supporting DSP dependencies are used where they make sense, instead of forcing a single external library for everything
## Requirements

- Rust toolchain installed
- CMake installed
- A supported environment on Windows, macOS, or Linux
- A valid audio device name configured in `setup.yaml`
- NAM model files and captures available under `captures/`
- IR files available when a block enables `ir_path`

## How to Run

1. Review `setup.yaml` and update `match_name` so it matches your local audio interface name exactly or by substring.
2. Confirm the configured sample rate and buffer size are supported by your device.
3. Make sure the NAM model paths referenced in `setup.yaml` exist on disk.
4. Run a compile check:

```bash
cargo check
```

5. Start the console adapter:

```bash
cargo run -p adapter-console
```

## Current Runtime Notes

- The first working path is mono per track.
- NAM blocks are processed in sequence.
- NAM blocks can apply input/output gain, embedded noise gate, embedded EQ, and IR in the audio path.
- The process keeps running until you stop it manually.

## Main Files

- `setup.yaml`: current runtime setup and device/model references
- `state.yaml`: current logical state snapshot
- `crates/adapter-console/src/main.rs`: console bootstrap
- `crates/infra-cpal/src/lib.rs`: CPAL device and stream integration
- `crates/infra-yaml/src/lib.rs`: YAML loader
- `crates/engine/src/runtime.rs`: simple runtime queue and sample processing
- `crates/stage-amp-nam/src/processor.rs`: native NAM processor binding
