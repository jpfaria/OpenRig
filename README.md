# OpenRig

OpenRig is a Rust guitar and acoustic pedalboard project built to grow from a simple working audio path into a professional multi-adapter architecture.

It is intended to run on Windows, macOS, and Linux.

## Direction

- Core-first design
- Clear separation between setup, application, engine, infrastructure, and adapters
- Generic audio blocks instead of a NAM-only model
- NAM, IR, and internal effects treated as distinct concepts
- First priority is a simple working path end to end

## Current Vertical Slice

The current implementation is focused on a minimal working path:

- Load setup from YAML
- Validate the setup structure
- Resolve audio devices with CPAL
- Load NAM processors through the native wrapper
- Build a simple per-track runtime queue
- Run audio input and output streams from the console adapter

## Stage Catalog
Implemented today:
- `stage-nam`: NAM model processing
- `stage-delay`: digital delay
- `stage-reverb`: plate reverb foundation
- `stage-utility`: chromatic tuner
- `stage-dynamics`: compressor and noise gate
- `stage-eq`: three-band EQ
- `stage-modulation`: tremolo
Planned next expansions:
- `stage-delay`: tape, analog, dual, ping-pong
- `stage-reverb`: spring, hall, room as distinct algorithms
- `stage-modulation`: chorus, phaser, flanger, rotary
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
- IR paths are already part of the setup, but IR processing is not applied in the audio path yet.
- The process keeps running until you stop it manually.

## Main Files

- `setup.yaml`: current runtime setup and device/model references
- `state.yaml`: current logical state snapshot
- `crates/adapter-console/src/main.rs`: console bootstrap
- `crates/infra-cpal/src/lib.rs`: CPAL device and stream integration
- `crates/infra-yaml/src/lib.rs`: YAML loader and compatibility mapping
- `crates/engine/src/runtime.rs`: simple runtime queue and sample processing
- `crates/stage-nam/src/processor.rs`: native NAM processor binding
