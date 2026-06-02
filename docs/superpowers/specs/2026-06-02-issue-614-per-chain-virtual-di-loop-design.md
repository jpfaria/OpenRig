# Per-chain virtual DI loop (issue #614)

## Goal

Let the user shape tone (timbrar) without playing. Inject a pre-decoded guitar
**DI (dry) loop** in place of the live device input of a single chain, looping
forever, so it passes through that chain's full block graph (amp/cab/pedals)
exactly like a live signal.

Ephemeral (runtime-only): never written to the project file. This is a
tone-shaping aid, not routing (ADR 0003).

This is the "synthetic input into a chain" variant left open in #324. It is
distinct from #324 (output-side backing-track player with transport/pitch/speed
and a separate window).

## Non-goals

- Output-side mixing of a backing track, transport (seek/pause), pitch/speed —
  that is #324.
- Persisting the loop choice in `.openrig`.
- Broad codec support in v1 (WAV only; symphonia/mp3/flac can follow).

## Mechanism

Mirrors the existing latency-probe input injection: the probe already writes
into the input stream inside `process_input_f32` using atomics + ArcSwap to talk
to the audio thread without locking (`crates/engine/src/runtime_state.rs`, probe
fields ~L270-279; injection in `runtime.rs::process_input_f32`).

### 1. Audio thread (zero alloc/lock/syscall — invariant #8)

Two new fields on `ChainRuntimeState`:

```rust
/// Ephemeral per-chain DI loop. `None` = use the live device input.
/// Published off-thread via ArcSwap; read once per callback (lock-free).
di_loop: ArcSwap<Option<Arc<DiLoop>>>,
/// Playback cursor (frame index into DiLoop). Advanced by the audio thread.
di_loop_pos: AtomicUsize,
```

`DiLoop` is an immutable, preallocated, read-only buffer:

```rust
pub struct DiLoop {
    /// Interleaved-by-frame stereo samples at the engine sample rate.
    /// Mono sources are stored as a single channel; broadcast happens at read.
    frames: Box<[DiFrame]>,        // DiFrame = mono f32 OR [f32; 2]
    layout: AudioChannelLayout,    // Mono | Stereo
    len: usize,                    // frames.len(), cached
}
```

In `process_input_f32`, before reading the device frame for a segment:

- `let loop_arc = self.di_loop.load();` (ArcSwap, ~10-20 ns; `None` fast-path
  returns to today's device read with one branch).
- If `Some`: read `pos = di_loop_pos.load(Relaxed)`, take `frames[pos]`, produce
  the `AudioFrame` (mono → `Stereo([s, s])` broadcast, invariant #5; stereo →
  direct), `di_loop_pos.store((pos + 1) % len, Relaxed)`.
- The produced `AudioFrame` replaces the result of `read_input_frame(...)` for
  that segment. Everything downstream (taps, DSP, mixdown) is unchanged.

Position advances once per output frame consumed. With one chain runtime owning
one `di_loop_pos`, all of that chain's input segments read the same cursor — the
whole chain hears the same DI, which is the intended "play this chain with a
virtual guitar" behavior.

When off (`None`): byte-identical to today's device passthrough.

### 2. Preload (off the audio thread)

Triggered by the source-select command. Runs in the command side-effect / a
worker — never on RT:

1. Decode the file. Reuse `crates/adapter-render/src/wav.rs` for WAV.
2. Resample once to the engine sample rate (engine SR is known at runtime build;
   plumb it to the preload). Use a windowed-sinc / good-quality resample done
   once offline (quality > speed here, it is not on RT). Evaluate reusing an
   existing resampler in the workspace before adding a dependency.
3. Equal-power crossfade at the loop seam: blend the tail into the head over a
   short window (e.g. 5-20 ms) so wrap-around produces no click (invariant #3).
4. Build `Arc<DiLoop>` and publish with `di_loop.store(Some(arc))`; reset
   `di_loop_pos` to 0.

Clearing the source / disabling: `di_loop.store(None)`.

### 3. Commands (transport-agnostic; MCP/gRPC parity automatic)

In `crates/application/src/command.rs`:

- `SetChainDiLoopSource { chain: ChainId, source: DiLoopSource }` — selects and
  loads the loop (bundled id or file path). Decode/resample happens in the
  side-effect; on success it publishes the `Arc<DiLoop>` to the chain runtime.
- `SetChainDiLoopEnabled { chain: ChainId, enabled: bool }` — play/stop. Enable
  with no source loaded is a no-op (or selects the last source). Disable stores
  `None`.

`DiLoopSource` enum: `Bundled(BundledDiId)` | `File(PathBuf)`.

GUI dispatches via `dispatcher.dispatch`; MCP/gRPC inherit the same variants
(parity law). No `borrow_mut()` in callbacks.

### 4. Source files

- `assets/di-loops/` with 1-2 **CC0** guitar DI loops (sourced separately; each
  file's license shown to the user before committing).
- User-picked file via the existing file-picker plumbing.

### 5. UI

Slint is a pure dispatcher: callback → `Event` → pure function → `Command`.

Placement: an **icon button next to the per-chain volume control**. Interaction:

- Click the icon → reveals a select (dropdown) to choose the loop ("música") +
  a play button beside it.
- Click play → dispatches `SetChainDiLoopSource` (if needed) + enable; the
  button swaps to a **stop** button.
- Click stop → dispatches disable; back to play.

Icons are SVG via `@image-url` + colorize (never glyphs — Orange Pi tofu). Keep
visual consistency with the existing chain-tile controls.

## Invariants (must not regress)

- Per-chain isolation: override lives on one `ChainRuntimeState`; other chains
  untouched (#4).
- Stereo-internal + mono broadcast preserved (#5).
- Zero alloc/lock/syscall on the audio thread: ArcSwap load + atomic index +
  array read only (#8).
- Off = byte-identical device passthrough; determinism / golden samples
  unaffected when off (#9).
- No new xrun/dropout/click; the loop seam is crossfaded (#3).

## Testing (TDD red-first)

Engine (`crates/engine`):
- Injection replaces the device frame when a `DiLoop` is set; off = passthrough.
- Mono `DiLoop` read produces `Stereo([s, s])`; stereo reads direct.
- Cursor wraps at `len`; sequence is contiguous across the wrap.
- No allocation on the audio thread with a loop active (existing
  alloc-invariant harness).
- Isolation: a `DiLoop` on chain A does not affect chain B's input.

Preload:
- Resample to engine SR yields the expected length/SR.
- Equal-power crossfade: seam sample continuity (no discontinuity > threshold)
  across wrap.

Application (`crates/application`):
- `SetChainDiLoopSource` / `SetChainDiLoopEnabled` flow through the dispatcher
  and reach the runtime (pure-function event handlers, no `AppWindow`).

## Build sequence (high level; detailed plan to follow)

1. RED tests for the engine injection + `DiLoop` read semantics → implement
   the `ChainRuntimeState` fields + `process_input_f32` branch.
2. RED tests for preload (decode→resample→crossfade) → implement preload.
3. RED tests for the commands → add variants + dispatch + side-effect.
4. UI: icon-next-to-volume + select + play/stop, wired to the commands.
5. Bundle CC0 DI loop(s) (license-approved).
6. Docs: `docs/screens.md` (Chains), `docs/audio-config.md` if relevant.

## Open items

- Which CC0 DI files to bundle (license review with the user).
- Confirm whether a resampler already exists in the workspace before adding one.
