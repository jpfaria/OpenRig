# `openrig-render` — headless offline render

`openrig-render` is the headless console binary shipped by
`crates/adapter-render` (issue #552). It applies a chain/preset to an
input WAV (or captures live from your interface) and writes the
processed result to an output WAV. No GUI, no MCP, no MIDI. Same
`engine::offline::render_chain` that the realtime callback uses — a
chain rendered offline is byte-identical to what the live rig would
emit for the same input samples.

Built for the audio-validation pipeline (`openrig-tone-analyzer` skill,
OpenRig-claude#8): you record a DI once, the renderer applies whatever
chain we built, the analyzer compares the result to the original song.
Iterate without re-recording.

## Usage

```
cargo run -p adapter-render -- \
  --chain  CHAIN.yaml \
  --input  DI.wav \
  --output WET.wav \
  [--start S] [--end E] \
  [--duration N --input-device DEV] \
  [--sample-rate Hz] [--block-size N] [--bit-depth 16|24|32] [--tail-ms N]
```

## Flags

| Flag | Default | Meaning |
|---|---|---|
| `--chain <path>` | required | Chain/preset YAML — same shape as `presets/clean.yaml` (a flat `blocks:` list, no I/O blocks). Synthesized into a `Chain` internally and fed to `engine::offline::render_chain`. |
| `--input <path>` | required | WAV path. **Path-based dual mode** (see below): existing file → file source; missing file → capture target. |
| `--output <path>` | required | Output WAV. Written atomically via `<path>.tmp` + rename; a failed render leaves no partial `<path>` behind. |
| `--start <S>` | none | File mode only. Skip the first `S` seconds of the input WAV. |
| `--end <E>` | none | File mode only. Stop at `E` seconds of the input WAV. `--start 5.0 --end 15.0` keeps a 10-second slice. |
| `--duration <N>` | none | Live capture mode only. Capture from the input device for `N` seconds. |
| `--input-device <name>` | default | cpal input device. Substring match against device names; `None` → default input device. |
| `--sample-rate <Hz>` | `48000` | Engine sample rate. |
| `--block-size <frames>` | `256` | Internal process block size. Should not change observable output for time-domain-stable blocks. |
| `--bit-depth <16\|24\|32>` | `24` | Output WAV sample format. `32` = 32-bit float; `16`/`24` = signed PCM. |
| `--tail-ms <ms>` | `2000` | Extra silence appended after the input so reverb/delay tails are captured instead of truncated. |

## File mode vs live-capture mode

The renderer picks its mode based on whether `--input` already exists:

* **`--input` exists** → **file mode**. Reads the WAV, optionally slices with
  `--start`/`--end`, processes through the chain. `--duration` and
  `--input-device` are ignored.
* **`--input` does not exist** → **live capture mode**. Opens
  `--input-device` (or the default cpal input device), captures for
  `--duration` seconds, **saves the dry capture to `--input` path**, then
  processes through the chain. On the next run, `--input` exists, so it
  re-uses the saved capture — you don't have to play the phrase again.

This is the path-based cache pattern: capture once, iterate forever. Pair
it with versioned `--output` paths (e.g. `wet_v1.wav`, `wet_v2.wav`) to
A/B different chain tweaks against the same dry take.

If `--input` does not exist and `--duration` is also missing, the
renderer exits with `1` and the message
`input wav <path> does not exist; pass --duration to capture from interface`.

## Exit codes

| Code | Meaning |
|---|---|
| `0` | Render succeeded; `--output` written |
| `1` | Render failed (bad chain, bad input WAV, capture failed, engine error, IO error). No partial output file remains |
| `2` | Argument error (missing required flag, invalid value such as `--bit-depth 19`, unknown flag) |

## Scope

Single chain. No multi-chain mixdown. No I/O block routing — the offline
driver supplies the input bus and consumes the output bus directly, so
any `InputBlock` / `OutputBlock` device wiring would be ignored. No
MIDI/automation replay. Score-driven rendering (synthesise audio from a
MIDI score and run it through the chain) is a deferred follow-up.

## Determinism

Same chain + same input WAV + same `--sample-rate --block-size --bit-depth`
produces a byte-identical output WAV. Pinned by
`crates/adapter-render/tests/issue_552_render_engine.rs::render_is_deterministic_byte_for_byte`.
Live capture is **not** deterministic (the interface's input changes
sample by sample) — once the DI is saved, every subsequent file-mode
re-render is.

## Why a separate binary

Headless by design. `adapter-render` must run on CI runners and Linux
boxes without a display server (and without a sound device too, in file
mode). Putting it on the GUI binary would force the headless build to
compile Slint, MCP, and MIDI even when none of them initialise. It
follows the same pattern as `adapter-console` and `adapter-console-rig`:
console-style adapters around the same engine core.
