# Issue #826 — waveform trim / crop / cut for a recorded loop

Status: approved design (2026-08-02)
Depends on: #323 (looper, shipped — single-take, controller-owned store)

## Goal

Let the player reshape a take that is nearly good instead of recording it
again: see the recorded loop as a waveform, select a region, and **trim** the
loop points, **crop** to the selection, or **cut** a region out and close the
gap. Control-thread only — no new DSP on the audio thread.

## Scope

In:

- Waveform view of the recorded loop, in a root-level modal overlay.
- Trim (drag the loop bounds inward), crop (keep the selection), cut (remove
  the selection and join the seam).
- Equal-power micro-crossfade at every seam, so an edit never clicks.
- An edit undo/redo stack owned by the editor, separate from the transport.
- `Command` variants, so MCP/gRPC get the same edits headlessly.

Out:

- Audio preview of a pending edit (apply, listen, undo if it is wrong).
- Grid/BPM snap, fade handles, per-layer editing, pitch/time-stretch,
  multi-clip arrangement.

## Where the loop actually lives (post-#323 redesign)

The looper is **single-take** and its slot lives in the controller-owned
`infra-cpal::looper_store::LooperStore` on the control thread, not behind the
audio-thread op queue:

- read: `LooperStore::export(chain, uid) -> Option<Vec<f32>>` (interleaved
  stereo, one loop long) — exposed as
  `Controller::export_chain_looper(chain_id, uid)`.
- write: `LooperStore::load(chain, uid, pcm)` → `LooperSlot::load_layer`, which
  lands the slot in `Stopped` and bumps `content_rev`; the isolated playback
  stream re-arms off that revision.

So an edit is a pure control-thread read → transform → write. Nothing new is
pushed to the audio thread, and no allocation happens in the callback path
(`load_layer` retires the old buffer on the control thread, as the
restore-from-disk path already does).

`LooperAction::Undo`/`Redo` are deliberate no-ops for a single-take looper;
the editor's undo stack (below) is a separate thing and does not change them.

### Gotcha: the export must be raw

`LooperSlot::export_mixdown` applies `mix`, `decay` and `reverse` — right for
writing a wav, **wrong** for edit-and-reinstall: re-installing that buffer would
bake the loop level and the reverse flag into the audio and then apply them a
second time on playback. The editor therefore reads a new
`LooperSlot::export_raw()` — the recorded material as captured, ignoring
`mix`/`decay`/`reverse`, `len_frames` long — and the store exposes it as
`LooperStore::export_raw(chain, uid)`. `export_mixdown` is untouched, so the
project-save path keeps its current bytes (pinned by the existing tests).

## Components

### 1. `application/src/looper_edit.rs` — pure edit core (new)

No I/O, no engine types; operates on an interleaved-stereo `&[f32]`.

```rust
pub enum LoopEdit {
    /// Keep [start, end) — the loop bounds move inward.
    Trim { start: usize, end: usize },
    /// Keep [start, end) and discard the rest. Same transform as Trim; kept
    /// as a distinct variant so the intent survives into MCP and the log.
    Crop { start: usize, end: usize },
    /// Drop [start, end) and join the two halves.
    Cut { start: usize, end: usize },
}

pub fn apply_edit(pcm: &[f32], edit: LoopEdit, seam_frames: usize)
    -> Result<Vec<f32>, EditError>;

/// Bucketed peak envelope for drawing: `buckets` values in 0..=1, each the
/// peak absolute sample of both channels in that slice.
pub fn peaks(pcm: &[f32], buckets: usize) -> Vec<f32>;
```

Rules:

- Frame indices, not samples; `start < end`, `end <= len`, out-of-range or an
  empty/whole-loop result is `EditError` (never a panic, never a silent clamp).
- The result must keep at least `MIN_LOOP_FRAMES` (one seam's worth ×2) and at
  most the 60 s ceiling (`LooperSlot::max_frames`, enforced by the caller that
  knows the sample rate).
- Seam handling: the **equal-gain overlap-add** of
  `engine::di_loop::apply_loop_crossfade` (#614 — equal-gain weights sum to 1,
  so the seam never overshoots the source peak; equal-power would). The result
  of a trim/crop wraps end→start continuously; a cut blends the two halves at
  the join. `seam_frames = min(SEAM_FRAMES_DEFAULT, region / 4)`, and the
  transform is a no-op fade when the region is too short to spare the overlap.

### 2. Commands (`application/src/command/looper.rs`)

```rust
EditChainLooperAudio { chain: ChainId, looper: u64, edit: LoopEdit },
UndoChainLooperEdit  { chain: ChainId, looper: u64 },
RedoChainLooperEdit  { chain: ChainId, looper: u64 },
```

Layering follows the existing looper split (the #614 rule "a dispatch alone is
dead"):

- `local_dispatcher_looper.rs` stays project-side and pure: it validates that
  the chain and looper exist and emits `Event::ChainLooperEditApplied` /
  `…EditUndone` / `…EditRedone`. A recording is runtime state, so no project
  field changes — exactly like the transport commands.
- `adapter-gui::looper_wiring::apply_looper_event` turns the event into a
  controller call, so MCP/MIDI reach the store through the same drain the GUI
  uses (the parity law).
- `infra-cpal::Controller::looper_apply_edit(chain, uid, edit) ->
  Result<usize, LooperEditError>` does the work on the control thread: refuse
  unless the slot is `Stopped`, `export_raw`, `apply_edit`, push the pre-edit
  buffer onto the history, `store.load`, return the new length. Editing
  mid-record/mid-play is an error, never a silent stop.
- The wav is NOT rewritten inline. An edit marks the project dirty and the
  existing `looper_persist::save_chain_loops` writes the edited loop on save,
  the one path that already owns the project path.

### 3. Edit history

A per-slot stack inside `LooperStore` (control thread, where the buffers
already live): a `Vec<Vec<f32>>` of pre-edit buffers plus a redo stack, capped
at `LOOPER_EDIT_HISTORY_MAX = 8` (a 60 s stereo loop at 48 k is ~23 MB, so the
cap is memory, not taste — the oldest entry drops first). Cleared on
record/clear/remove, and gone with the slot on project close.

## UI

A root-level overlay component `looper_editor_overlay.slint` — never a
`PopupWindow` (#749/#761: its content never receives clicks) — opened from an
edit button in the looper row, closed by a backdrop click or Esc.

- Full-width waveform: a row of `WAVEFORM_BUCKETS = 256` bars driven by an
  `in property <[float]>` of peaks computed in Rust. Bars, not a `Path`, so it
  stays themeable and testable, and so no drawing code has to be invented.
- Two draggable handles bound the selection; the region outside is dimmed. The
  selection is reported back to Rust as two float ratios (0..1), which the
  handler converts to frame indices — the UI never learns about frames.
- Buttons: `Trim`, `Crop`, `Cut`, `Undo`, `Redo`, `Close`. Trim/Crop/Cut are
  disabled without a selection; Undo/Redo follow the stack depth.
- No playhead (the editor is stopped-only) and no audio preview.
- Slint stays a dispatcher: each button emits a callback that
  `looper_callbacks.rs` turns into a `Command`.

## Testing

Red-first, in this order:

1. `looper_edit` unit tests: trim/crop/cut frame maths on a synthetic ramp
   (exact expected samples), seam fade shape, every `EditError` case, `peaks`
   bucketing including a loop shorter than the bucket count.
2. `export_raw` returns the captured material with `mix`/`reverse` set to
   non-default values — the regression that stops level/reverse being baked in;
   plus the existing `export_mixdown` tests unchanged (they pin the save path).
3. Dispatcher tests: each edit command emits its event (unknown chain/looper is
   an error). Store/controller tests: edit while playing is rejected; edit while
   stopped installs the new length and fills the history; undo restores the
   previous buffer sample-for-sample; the cap drops the oldest; recording clears
   the history.
4. GUI: a `slint-render` PNG of the overlay (visual proof) **and** an
   `i-slint-backend-testing` interaction test that drags a handle and clicks
   Trim, asserting the dispatched command (#749 lesson: rendering alone proves
   nothing about clicks).
5. No audio-thread test changes: the callback path is untouched. Invariants
   #1–#10 are unaffected by construction — the only engine change is a new
   read-only method plus an existing `load_layer` call.

## Docs

`docs/screens.md` (the editor overlay), `docs/blocks-catalog.md` if the looper
row gains a control, and the MCP tool list — same commit as the code.
