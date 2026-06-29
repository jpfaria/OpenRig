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
| `1` | Render failed (bad chain, bad input WAV, capture failed, engine error, **one or more chain blocks could not be built**, IO error). No partial output file remains |
| `2` | Argument error (missing required flag, invalid value such as `--bit-depth 19`, unknown flag) |

### Failing-block policy (issue #574)

`engine::offline::render_chain` is best-effort: when an individual block
fails to build at setup time (missing plugin file, unresolvable model
id, invalid params), the engine replaces it with a pass-through bypass
node and keeps rendering. The GUI relies on this — the user must be
able to keep working with a partial chain. **The CLI does not inherit
that policy.** If any block in the chain ends up bypassed because it
could not be built, `openrig-render` exits with code `1` and stderr
lists every failing block:

```
openrig-render: 1 block(s) in the chain failed to build and would have been silently bypassed:
  - block 'preset:Coldplay - Clocks (rhythm):block:3' (nam/nam_fender_twin_reverb): missing or invalid string parameter 'model_path'
refusing to write a WAV that would be missing those blocks' contribution
```

Before #574 the same render exited `0` with a WAV that silently omitted
the failing blocks' contribution — two different presets could produce
byte-identical output. Refusing to claim success is the only honest
outcome for an offline render.

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

Shipped in every platform package as `openrig-render` (issue #741),
alongside the headless `openrig-console` / `openrig-console-rig` and the
GUI `openrig` — so an installed OpenRig can render offline without a
separate `cargo run`. The packagers build and stage these from
`scripts/lib/console-binaries.tsv` (the single source of truth).

## Render as a `Command` (issue #576)

Offline render is exposed as `Command::RenderChain`. It does not mutate
the live project's State; it sits on the command bus purely for
transport-adapter parity — the LEI "every user operation is a
`Command`" applies here too, so MCP/gRPC/any future transport inherits
the tool automatically through `application::command_schema`. The
handler in `application::render_handler` is a thin shim that calls
`adapter_render::render()` (the same call site as the `openrig-render`
binary, same atomic output write, same determinism guarantee).

**File mode only.** Live capture (cpal-driven) stays inside the
`openrig-render` binary so `application` does not pick up a sound-device
dep. An MCP/gRPC client that needs a live capture re-uses the binary
out-of-band, then re-runs the command with the captured WAV.

### MCP tool: `render_chain`

Auto-derived from the `Command::RenderChain` variant — no hand-written
schema, no special-case wiring in `adapter-mcp`. The tool appears in the
catalog alongside every other Command-tool.

Input shape (mirrors the file-mode flags above; live-capture flags are
intentionally absent — use the binary for that):

```jsonc
{
  "chain_path":   "/abs/path/chain.yaml",   // required
  "input_path":   "/abs/path/di.wav",       // required (must exist on disk)
  "output_path":  "/abs/path/wet.wav",      // required
  "start_s":      0.0,                       // optional
  "end_s":        12.34,                     // optional
  "sample_rate_hz": 48000,                   // optional, default 48000
  "block_size":     256,                     // optional, default 256
  "bit_depth":      24,                      // optional, 16|24|32, default 24
  "tail_ms":      2000                       // optional, default 2000
}
```

The tool call returns the dispatcher's event list as JSON; the
interesting event is `RenderCompleted`:

```jsonc
{
  "RenderCompleted": {
    "output_path":      "/abs/path/wet.wav",
    "duration_seconds": 12.34,
    "sample_rate":      48000,
    "bit_depth":        24
  }
}
```

Failures (invalid `bit_depth`, missing input WAV, chain load error,
engine error, IO error) come back as a `CallToolResult` error with the
underlying message. No partial output WAV is left on failure — the
atomic `<output>.tmp` + rename guarantee carries through unchanged from
the binary.

Paths are local to the host. The tool does **not** stream audio over
MCP — host and client are assumed co-located. Same scope limits as the
binary (single chain, no I/O block routing, no MIDI/automation replay).
The `openrig-render` binary stays — the Command path is an additional
surface for agents/transports, not a substitute.
