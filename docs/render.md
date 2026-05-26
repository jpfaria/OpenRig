# `openrig-render` — headless offline render

`openrig-render` is the headless console binary shipped by
`crates/adapter-render` (issue #552). It loads a `.openrig` project, pumps an
input WAV through the chain's DSP, and writes the result to an output WAV.
No GUI, no audio device, no MCP, no MIDI. Same `engine::offline::render_chain`
that the realtime callback uses for `RuntimeProcessor::process_buffer` — so a
chain rendered offline is by construction byte-identical to what the live rig
would emit for the same input samples.

Used by the audio-validation pipeline (`openrig-tone-analyzer` skill,
OpenRig-claude#8) to render a DI through a built preset and compare the
result against a reference recording.

## Usage

```
openrig-render --project P.openrig --input IN.wav --output OUT.wav
              [--chain ID_OR_DESCRIPTION]
              [--sample-rate 48000]
              [--block-size 256]
              [--bit-depth 16|24|32]
              [--tail-ms 2000]
```

| Flag | Default | Meaning |
|---|---|---|
| `--project <path>` | required | `.openrig` project file (or legacy chain YAML — migrated transparently) |
| `--input <path>` | required | Input WAV (8/16/24/32-bit PCM or 32-bit float; mono or stereo) |
| `--output <path>` | required | Output WAV path. Written atomically via `<path>.tmp` + rename — a failed render leaves no partial file |
| `--chain <id_or_description>` | first chain | Pick a specific chain from the project; matches either the chain id or its `description` field |
| `--sample-rate <Hz>` | `48000` | Output sample rate. Input WAVs are read at their native rate; the engine processes at this rate |
| `--block-size <frames>` | `256` | Internal chunk size. Should not change observable output for time-domain-stable blocks |
| `--bit-depth <16\|24\|32>` | `24` | Output sample format. `32` = 32-bit float; `16`/`24` = signed PCM |
| `--tail-ms <ms>` | `2000` | Extra silence appended after the input so reverb/delay tails are captured instead of truncated |

## Exit codes

| Code | Meaning |
|---|---|
| `0` | Render succeeded; `--output` written |
| `1` | Render failed (bad project file, bad input WAV, engine build error, IO error). No partial output file remains |
| `2` | Argument error (missing required flag, invalid value such as `--bit-depth 19`, unknown flag) |

## Scope

Single chain. No multi-chain mixdown. No I/O block routing — the offline
driver supplies the input bus and consumes the output bus directly, so any
`InputBlock` / `OutputBlock` device wiring in the project is ignored. No
MIDI replay, no automation. Multi-chain rendering remains out of scope
because the audio-validation pipeline operates one preset at a time.

## Determinism

Same project + same input WAV + same `--sample-rate --block-size --bit-depth`
produces a byte-identical output WAV. Pinned by
`crates/adapter-render/tests/issue_552_render_engine.rs::render_is_deterministic_byte_for_byte`.
This is required for the analyzer's compare step to produce repeatable
match scores across runs.

## Why a separate binary

`openrig-render` is headless by design — it must run on CI runners and
Linux boxes without an audio device or display server. Putting it on the GUI
binary would force the headless build to compile Slint, cpal, and the MIDI
stack even when none of them initialise. Keeping it separate also matches
OpenRig's existing pattern: `adapter-console`, `adapter-console-rig`, and
now `adapter-render` are all console-style adapters around the same engine
core.
