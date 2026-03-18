# OpenRig

OpenRig is a Rust guitar and acoustic pedalboard project built to grow from a simple working audio path into a professional multi-adapter architecture.

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

## Requirements

- Rust toolchain installed
- CMake installed
- A macOS environment with the CoreAudio frameworks available
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
cargo run -p pedal-adapter-console
```

## Current Runtime Notes

- The first working path is mono per track.
- NAM blocks are processed in sequence.
- IR paths are already part of the setup, but IR processing is not applied in the audio path yet.
- The process keeps running until you stop it manually.

## Main Files

- `setup.yaml`: current runtime setup and device/model references
- `state.yaml`: current logical state snapshot
- `crates/pedal-adapter-console/src/main.rs`: console bootstrap
- `crates/pedal-infra-cpal/src/lib.rs`: CPAL device and stream integration
- `crates/pedal-infra-yaml/src/lib.rs`: YAML loader and compatibility mapping
- `crates/pedal-engine/src/runtime.rs`: simple runtime queue and sample processing
- `crates/pedal-nam/src/processor.rs`: native NAM processor binding
