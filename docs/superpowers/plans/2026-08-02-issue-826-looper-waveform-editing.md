# Looper waveform trim / crop / cut — Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Let the player select a region of a recorded loop and trim / crop / cut it, with click-free seams and an undo stack, from the GUI and from MCP.

**Architecture:** The loop lives in the controller-owned `LooperStore` on the control thread (post-#323 redesign; the audio thread is not involved). An edit is read (`export_raw`) → transform (pure `looper_edit`) → write (`LooperStore::load` → `LooperSlot::load_layer`). A new `Command` carries the edit, the dispatcher emits an event, and the adapter wiring applies it to the store — the same bus MCP and MIDI travel. The UI is a root-level modal overlay drawing a bucketed peak envelope computed in Rust.

**Tech Stack:** Rust (workspace crates `engine`, `application`, `infra-cpal`, `adapter-gui`), Slint UI, `cargo test`, `tools/slint-render` for headless PNGs, `i-slint-backend-testing` for interaction tests.

Spec: `docs/superpowers/specs/2026-08-02-issue-826-looper-waveform-editing-design.md`.

## Global Constraints

- Branch `feature/issue-826`, cut from `release/v0.3.0`. Work ONLY inside `.solvers/issue-826/`. Stage explicit paths, never `git add -A`. Push after every commit, then `gh issue comment 826`.
- TDD red-first is mandatory: write the test, RUN it, see it FAIL with the assertion (not just a compile error), then implement. Never weaken a test to make it pass.
- Zero warnings: `cargo build` and `cargo clippy` must be clean.
- Repo content in English (code comments, docs, commit messages).
- Nothing in this feature may touch the audio thread. No allocation, lock or I/O is added to any `tick`/callback path. Invariants #1–#10 of `CLAUDE.md` must not regress.
- Do not modify `LooperSlot::export_mixdown` behaviour — the project-save bytes are pinned by existing tests.
- New user-visible strings use `@tr(...)` and must be extracted into all 9 translation catalogs in the same commit (`scripts/extract-translations.sh`).
- Never use `PopupWindow` for the editor (its content does not receive clicks — #749/#761). Root-level overlay only.
- Run tests with `cargo test -p <crate>`; the full suite before the final push.

---

### Task 1: Raw export from the looper slot

The editor must read the recorded material WITHOUT `mix`/`decay`/`reverse` applied. `export_mixdown` bakes them in, which is right for a wav and wrong for re-installing the buffer (level and reverse would be applied twice).

**Files:**
- Modify: `crates/engine/src/looper.rs` (add `export_raw` next to `export_mixdown`, ~line 301)
- Test: `crates/engine/src/looper_tests.rs`

**Interfaces:**
- Consumes: nothing.
- Produces: `LooperSlot::export_raw(&self) -> Option<Vec<f32>>` — interleaved stereo, `len_frames` long, `None` when nothing is recorded.

- [ ] **Step 1: Write the failing test**

In `crates/engine/src/looper_tests.rs` (helpers `slot()`, `spare()`, `feed()` already exist at the top of the file):

```rust
#[test]
fn export_raw_ignores_level_and_reverse() {
    // #826: the waveform editor re-installs what it exports. If the export
    // applied mix/reverse, an edit would bake them into the audio and playback
    // would apply them a second time.
    let mut s = slot();
    s.tap_record(Some(spare(MAX)));
    feed(&mut s, &[[1.0, 1.0], [2.0, 2.0], [4.0, 4.0]]);
    s.tap_record(None); // freeze
    s.set_mix(0.5);
    s.set_reverse(true);

    assert_eq!(
        s.export_raw().expect("there is material"),
        vec![1.0, 1.0, 2.0, 2.0, 4.0, 4.0],
        "the raw export is the captured material, in order, at unity"
    );
    assert_eq!(
        s.export_mixdown().unwrap(),
        vec![2.0, 2.0, 1.0, 1.0, 0.5, 0.5],
        "the mixdown path is unchanged"
    );
}

#[test]
fn export_raw_sums_the_audible_layers_and_is_none_when_empty() {
    let mut s = slot();
    assert!(s.export_raw().is_none(), "nothing recorded yet");

    s.tap_record(Some(spare(MAX)));
    feed(&mut s, &[[1.0, 1.0], [2.0, 2.0]]);
    s.tap_record(None);
    s.tap_record(Some(spare(MAX)));
    feed(&mut s, &[[0.5, 0.5], [0.25, 0.25]]);
    s.tap_record(None);
    s.undo();

    assert_eq!(
        s.export_raw().unwrap(),
        vec![1.0, 1.0, 2.0, 2.0],
        "an undone layer is not audible, so it is not exported"
    );
}
```

- [ ] **Step 2: Run the tests and watch them fail**

Run: `cargo test -p engine export_raw`
Expected: compile error `no method named export_raw` — then, after Step 3 stubs it, the assertion must be what fails if the maths is wrong. (A compile error alone is not the RED; if it compiles, the assertion must fail first.)

- [ ] **Step 3: Implement**

In `crates/engine/src/looper.rs`, right after `export_mixdown`:

```rust
    /// The recorded material exactly as captured — the audible layers summed,
    /// with NO `mix`, `decay` or `reverse` — one loop long, interleaved stereo.
    ///
    /// This is what the waveform editor (#826) reads and re-installs. Exporting
    /// the mixdown instead would bake the loop level and the reverse flag into
    /// the buffer, and playback would then apply them a second time.
    ///
    /// Allocates: CONTROL THREAD ONLY.
    pub fn export_raw(&self) -> Option<Vec<f32>> {
        if self.active == 0 || self.len_frames == 0 {
            return None;
        }
        let mut out = Vec::with_capacity(self.len_frames * 2);
        for frame in 0..self.len_frames {
            let mut acc = [0.0f32; 2];
            for layer in 0..self.active {
                let buf = &self.layers[layer];
                acc[0] += buf[frame * 2];
                acc[1] += buf[frame * 2 + 1];
            }
            out.push(acc[0]);
            out.push(acc[1]);
        }
        Some(out)
    }
```

- [ ] **Step 4: Run the tests and verify they pass**

Run: `cargo test -p engine looper`
Expected: PASS, including every pre-existing `export_mixdown` test.

- [ ] **Step 5: Commit**

```bash
git add crates/engine/src/looper.rs crates/engine/src/looper_tests.rs
git commit -m "feat(#826): a looper exports its raw recorded material"
git push
```

---

### Task 2: The seam crossfade curve, defined once

`di_loop::apply_loop_crossfade` already owns the equal-gain overlap-add curve (#614: equal-gain weights sum to 1, so a seam never overshoots the source peak). The editor needs the same curve, so the weight becomes a shared function instead of a second copy.

**Files:**
- Create: `crates/engine/src/crossfade.rs`
- Modify: `crates/engine/src/lib.rs` (add `pub mod crossfade;` next to `pub mod di_loop;`, ~line 12)
- Modify: `crates/engine/src/di_loop.rs:215` (`apply_loop_crossfade` uses the shared weight)
- Test: `crates/engine/src/crossfade.rs` (inline `#[cfg(test)] mod tests`)

**Interfaces:**
- Consumes: nothing.
- Produces: `engine::crossfade::head_weight(i: usize, xfade: usize) -> f32` — the fading-in weight of overlap frame `i`; the fading-out partner is `1.0 - head_weight(i, xfade)`.

- [ ] **Step 1: Write the failing test**

At the bottom of the new `crates/engine/src/crossfade.rs`:

```rust
#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn head_weight_ramps_from_almost_zero_to_almost_one() {
        // #614's equal-gain ramp: i+1 over xfade+1, so the pair always sums to
        // 1 (no overshoot) and neither end sits exactly at 0 or 1.
        assert_eq!(head_weight(0, 3), 0.25);
        assert_eq!(head_weight(1, 3), 0.5);
        assert_eq!(head_weight(2, 3), 0.75);
    }

    #[test]
    fn the_pair_of_weights_always_sums_to_one() {
        for xfade in 1..16usize {
            for i in 0..xfade {
                let w = head_weight(i, xfade);
                assert!(
                    ((w + (1.0 - w)) - 1.0).abs() < f32::EPSILON,
                    "equal-gain: the seam must never overshoot the source peak"
                );
            }
        }
    }
}
```

- [ ] **Step 2: Run the test and watch it fail**

Run: `cargo test -p engine crossfade`
Expected: FAIL — the module does not exist yet.

- [ ] **Step 3: Implement**

`crates/engine/src/crossfade.rs`:

```rust
//! The seam crossfade curve, defined once (#614 shape, shared since #826).
//!
//! Equal-GAIN, not equal-power: the two weights sum to 1, so an overlap-add
//! seam can never overshoot the source peak — on a high-gain chain an
//! overshoot is an audible click that sounds like clipping every time the loop
//! wraps.

/// Weight of the fading-IN frame at overlap position `i` of `xfade` frames.
/// The fading-OUT partner takes `1.0 - head_weight(i, xfade)`.
#[inline]
pub fn head_weight(i: usize, xfade: usize) -> f32 {
    (i + 1) as f32 / (xfade + 1) as f32
}
```

Add `pub mod crossfade;` to `crates/engine/src/lib.rs` and replace the inline maths in `di_loop::apply_loop_crossfade` with `crate::crossfade::head_weight(i, xfade)`, keeping its surrounding comment.

- [ ] **Step 4: Run the tests and verify they pass**

Run: `cargo test -p engine crossfade && cargo test -p engine di_loop`
Expected: PASS — the DI loop tests still pass, proving the extraction changed no numbers.

- [ ] **Step 5: Commit**

```bash
git add crates/engine/src/crossfade.rs crates/engine/src/lib.rs crates/engine/src/di_loop.rs
git commit -m "refactor(#826): the seam crossfade curve lives in one place"
git push
```

---

### Task 3: The pure edit core

**Files:**
- Create: `crates/application/src/looper_edit.rs`
- Create: `crates/application/src/looper_edit_tests.rs`
- Modify: `crates/application/src/lib.rs` (add `pub mod looper_edit;` next to `pub mod looper_audio;`, ~line 56)

**Interfaces:**
- Consumes: `engine::crossfade::head_weight` (Task 2).
- Produces:
  - `enum LoopEdit { Trim { start: usize, end: usize }, Crop { start: usize, end: usize }, Cut { start: usize, end: usize } }` (frame indices, `Serialize`/`Deserialize`/`JsonSchema`/`Clone`/`Copy`/`Debug`/`PartialEq`)
  - `enum LoopEditError { OutOfRange, EmptyRegion, ResultTooShort }` (implements `std::fmt::Display` + `std::error::Error`)
  - `fn apply_edit(pcm: &[f32], edit: LoopEdit) -> Result<Vec<f32>, LoopEditError>`
  - `fn peaks(pcm: &[f32], buckets: usize) -> Vec<f32>`
  - `const MIN_LOOP_FRAMES: usize = 64;`
  - `const SEAM_FRAMES: usize = 64;`

- [ ] **Step 1: Write the failing tests**

`crates/application/src/looper_edit_tests.rs`:

```rust
//! #826 — the pure loop-edit transforms. Interleaved stereo, frame indices,
//! no I/O, no engine state.

use super::looper_edit::*;

/// `frames` ramp frames: frame i is [i, -i] so both channels and the frame
/// order are visible in every assertion.
fn ramp(frames: usize) -> Vec<f32> {
    (0..frames).flat_map(|i| [i as f32, -(i as f32)]).collect()
}

fn frame(pcm: &[f32], i: usize) -> [f32; 2] {
    [pcm[i * 2], pcm[i * 2 + 1]]
}

#[test]
fn trim_keeps_only_the_selected_frames() {
    let pcm = ramp(256);
    let out = apply_edit(&pcm, LoopEdit::Trim { start: 64, end: 192 }).unwrap();

    assert_eq!(out.len() / 2, 128 - SEAM_FRAMES, "the overlap is folded in, not kept");
    // The body (past the seam) is the source, frame for frame.
    assert_eq!(frame(&out, SEAM_FRAMES), frame(&pcm, 64 + SEAM_FRAMES));
}

#[test]
fn crop_is_the_same_transform_as_trim() {
    let pcm = ramp(256);
    let trimmed = apply_edit(&pcm, LoopEdit::Trim { start: 64, end: 192 }).unwrap();
    let cropped = apply_edit(&pcm, LoopEdit::Crop { start: 64, end: 192 }).unwrap();
    assert_eq!(trimmed, cropped, "crop and trim differ in intent, not in audio");
}

#[test]
fn cut_removes_the_region_and_joins_the_halves() {
    let pcm = ramp(300);
    let out = apply_edit(&pcm, LoopEdit::Cut { start: 100, end: 200 }).unwrap();

    assert_eq!(out.len() / 2, 200 - SEAM_FRAMES, "the removed region is gone");
    // Before the join, the head is untouched.
    assert_eq!(frame(&out, 0), frame(&pcm, 0));
    // After the join's overlap, the tail follows the source's post-cut frames.
    assert_eq!(frame(&out, 100 + SEAM_FRAMES), frame(&pcm, 200 + SEAM_FRAMES));
}

#[test]
fn the_seam_is_a_blend_not_a_step() {
    // The point of the seam: no frame at the join equals a raw source frame,
    // and nothing overshoots the source peak (equal-gain, #614).
    let pcm = ramp(300);
    let out = apply_edit(&pcm, LoopEdit::Cut { start: 100, end: 200 }).unwrap();
    let peak = pcm.iter().fold(0.0f32, |a, s| a.max(s.abs()));

    let joined = frame(&out, 100);
    assert_ne!(joined, frame(&pcm, 100), "the join is blended");
    assert!(out.iter().all(|s| s.abs() <= peak + 1e-3), "no overshoot at the seam");
}

#[test]
fn an_out_of_range_or_empty_region_is_an_error_not_a_panic() {
    let pcm = ramp(256);
    assert_eq!(
        apply_edit(&pcm, LoopEdit::Trim { start: 0, end: 999 }),
        Err(LoopEditError::OutOfRange)
    );
    assert_eq!(
        apply_edit(&pcm, LoopEdit::Trim { start: 100, end: 100 }),
        Err(LoopEditError::EmptyRegion)
    );
    assert_eq!(
        apply_edit(&pcm, LoopEdit::Trim { start: 200, end: 100 }),
        Err(LoopEditError::EmptyRegion)
    );
}

#[test]
fn an_edit_that_would_leave_almost_nothing_is_refused() {
    let pcm = ramp(256);
    assert_eq!(
        apply_edit(&pcm, LoopEdit::Trim { start: 0, end: 8 }),
        Err(LoopEditError::ResultTooShort)
    );
    assert_eq!(
        apply_edit(&pcm, LoopEdit::Cut { start: 8, end: 250 }),
        Err(LoopEditError::ResultTooShort)
    );
}

#[test]
fn peaks_bucket_the_envelope_into_zero_to_one() {
    // Frame i has amplitude i/255, so the last bucket peaks near 1.0 and the
    // first near 0.
    let pcm: Vec<f32> = (0..256)
        .flat_map(|i| {
            let a = i as f32 / 255.0;
            [a, -a]
        })
        .collect();
    let p = peaks(&pcm, 4);

    assert_eq!(p.len(), 4);
    assert!(p[0] < p[1] && p[1] < p[2] && p[2] < p[3]);
    assert!((p[3] - 1.0).abs() < 1e-3, "the loudest bucket reads 1.0");
    assert!(p.iter().all(|v| (0.0..=1.0).contains(v)));
}

#[test]
fn peaks_never_divides_by_zero_on_a_short_or_empty_loop() {
    assert_eq!(peaks(&[], 8), vec![0.0; 8]);
    assert_eq!(peaks(&ramp(2), 0), Vec::<f32>::new());
    assert_eq!(peaks(&ramp(3), 8).len(), 8, "fewer frames than buckets still fills them");
}
```

- [ ] **Step 2: Run the tests and watch them fail**

Run: `cargo test -p application looper_edit`
Expected: FAIL — module missing; once the stubs exist, the assertions must be the failing lines.

- [ ] **Step 3: Implement**

`crates/application/src/looper_edit.rs`:

```rust
//! #826 — reshaping a recorded loop: trim / crop / cut, on the control thread.
//!
//! Pure transforms over an interleaved-stereo buffer. Frame indices in, a new
//! buffer out. No engine state, no I/O — the caller (the controller) reads the
//! loop with `LooperSlot::export_raw` and installs the result with
//! `LooperSlot::load_layer`.

use engine::crossfade::head_weight;
use schemars::JsonSchema;
use serde::{Deserialize, Serialize};

/// Frames blended at a seam. ~1.3 ms at 48 kHz: long enough to kill the step,
/// short enough that nothing musical is smeared.
pub const SEAM_FRAMES: usize = 64;

/// Shortest loop an edit may leave behind — two seams' worth, so a seam always
/// has room and a stray drag cannot reduce a take to a click.
pub const MIN_LOOP_FRAMES: usize = SEAM_FRAMES * 2;

/// One reshaping of a recorded loop. Frame indices over the loop, `end`
/// exclusive.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
pub enum LoopEdit {
    /// Move the loop bounds inward: keep `[start, end)`.
    Trim { start: usize, end: usize },
    /// Keep `[start, end)` and discard the rest. The same audio transform as
    /// `Trim`; a distinct variant so the user's intent survives into MCP.
    Crop { start: usize, end: usize },
    /// Drop `[start, end)` and join the two halves.
    Cut { start: usize, end: usize },
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum LoopEditError {
    /// A bound lies outside the loop.
    OutOfRange,
    /// `start >= end`.
    EmptyRegion,
    /// The result would be shorter than `MIN_LOOP_FRAMES`.
    ResultTooShort,
}

impl std::fmt::Display for LoopEditError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::OutOfRange => write!(f, "the edit region is outside the loop"),
            Self::EmptyRegion => write!(f, "the edit region is empty"),
            Self::ResultTooShort => write!(f, "the edit would leave too little audio"),
        }
    }
}

impl std::error::Error for LoopEditError {}

impl LoopEdit {
    fn bounds(&self) -> (usize, usize) {
        match *self {
            Self::Trim { start, end } | Self::Crop { start, end } | Self::Cut { start, end } => {
                (start, end)
            }
        }
    }
}

/// Apply `edit` to an interleaved-stereo loop, returning the new loop.
///
/// Every result is seam-blended so playback wraps (and a cut joins) without a
/// step: the last `SEAM_FRAMES` are folded into the head with the equal-gain
/// overlap-add of #614, so the returned loop is `SEAM_FRAMES` shorter than the
/// naive selection.
pub fn apply_edit(pcm: &[f32], edit: LoopEdit) -> Result<Vec<f32>, LoopEditError> {
    let len = pcm.len() / 2;
    let (start, end) = edit.bounds();
    if start >= end {
        return Err(LoopEditError::EmptyRegion);
    }
    if end > len {
        return Err(LoopEditError::OutOfRange);
    }

    let kept: Vec<f32> = match edit {
        LoopEdit::Trim { .. } | LoopEdit::Crop { .. } => pcm[start * 2..end * 2].to_vec(),
        LoopEdit::Cut { .. } => {
            let mut out = Vec::with_capacity(pcm.len() - (end - start) * 2);
            out.extend_from_slice(&pcm[..start * 2]);
            out.extend_from_slice(&pcm[end * 2..]);
            out
        }
    };

    let kept_frames = kept.len() / 2;
    if kept_frames < MIN_LOOP_FRAMES + SEAM_FRAMES {
        return Err(LoopEditError::ResultTooShort);
    }
    Ok(seam_blend(&kept, SEAM_FRAMES))
}

/// Overlap-add the last `xfade` frames into the head, returning a buffer
/// `xfade` frames shorter. The new first frame is ~the source frame that
/// followed the new last frame, so the loop wraps continuously (#614).
fn seam_blend(pcm: &[f32], xfade: usize) -> Vec<f32> {
    let n = pcm.len() / 2;
    if xfade == 0 || n < xfade * 2 + 1 {
        return pcm.to_vec();
    }
    let m = n - xfade;
    let mut out = Vec::with_capacity(m * 2);
    for i in 0..xfade {
        let w = head_weight(i, xfade);
        for ch in 0..2 {
            out.push(pcm[i * 2 + ch] * w + pcm[(m + i) * 2 + ch] * (1.0 - w));
        }
    }
    out.extend_from_slice(&pcm[xfade * 2..m * 2]);
    out
}

/// Bucketed peak envelope for drawing: `buckets` values in 0..=1, each the
/// loudest absolute sample (either channel) in that slice of the loop.
/// A loop with fewer frames than buckets still fills every bucket.
pub fn peaks(pcm: &[f32], buckets: usize) -> Vec<f32> {
    if buckets == 0 {
        return Vec::new();
    }
    let frames = pcm.len() / 2;
    if frames == 0 {
        return vec![0.0; buckets];
    }
    (0..buckets)
        .map(|b| {
            let from = b * frames / buckets;
            let to = (((b + 1) * frames) / buckets).max(from + 1).min(frames);
            pcm[from * 2..to * 2]
                .iter()
                .fold(0.0f32, |a, s| a.max(s.abs()))
                .min(1.0)
        })
        .collect()
}

#[cfg(test)]
#[path = "looper_edit_tests.rs"]
mod tests;
```

Add `pub mod looper_edit;` to `crates/application/src/lib.rs`.

- [ ] **Step 4: Run the tests and verify they pass**

Run: `cargo test -p application looper_edit`
Expected: PASS (8 tests).

- [ ] **Step 5: Commit**

```bash
git add crates/application/src/looper_edit.rs crates/application/src/looper_edit_tests.rs crates/application/src/lib.rs
git commit -m "feat(#826): pure trim/crop/cut transforms for a recorded loop"
git push
```

---

### Task 4: The store applies, undoes and redoes an edit

**Files:**
- Modify: `crates/infra-cpal/src/looper_store.rs` (the entry struct, `export_raw`, `apply_edit`, `undo_edit`, `redo_edit`, history clearing in `tap_record`/`clear`/`remove`)
- Test: `crates/infra-cpal/src/looper_store_tests.rs`

**Interfaces:**
- Consumes: `LooperSlot::export_raw` (Task 1), `application::looper_edit::{LoopEdit, LoopEditError, apply_edit}` (Task 3), the existing `LooperStore::load`.
- Produces:
  - `LooperStore::export_raw(&self, chain: &ChainId, uid: u64) -> Option<Vec<f32>>`
  - `LooperStore::apply_edit(&mut self, chain: &ChainId, uid: u64, edit: LoopEdit) -> Result<usize, LooperEditRefused>` — the new length in frames
  - `LooperStore::undo_edit(&mut self, chain, uid) -> bool` / `redo_edit(&mut self, chain, uid) -> bool`
  - `LooperStore::edit_history_depth(&self, chain, uid) -> (usize, usize)` — (undo, redo), for the UI's button states
  - `enum LooperEditRefused { NotStopped, Empty, Unknown, Edit(LoopEditError) }`
  - `const LOOPER_EDIT_HISTORY_MAX: usize = 8;`

- [ ] **Step 1: Write the failing tests**

Append to `crates/infra-cpal/src/looper_store_tests.rs` (reuse the file's existing helpers for building a store and recording into a slot; if a helper records a loop, use it rather than duplicating the taps):

```rust
#[test]
fn an_edit_is_refused_while_the_loop_is_not_stopped() {
    // Editing is a stopped-only operation: a live loop must never be reshaped
    // under the player's feet, and never silently stopped either.
    let (mut store, chain, uid) = store_with_recorded_loop(512);
    store.play(&chain, uid);

    assert_eq!(
        store.apply_edit(&chain, uid, LoopEdit::Trim { start: 64, end: 448 }),
        Err(LooperEditRefused::NotStopped)
    );
    assert!(store.status(&chain, uid).is_some_and(|s| s.len_frames == 512),
        "the refused edit changed nothing");
}

#[test]
fn a_trim_on_a_stopped_loop_installs_the_shorter_loop() {
    let (mut store, chain, uid) = store_with_recorded_loop(512);
    store.stop(&chain, uid);

    let new_len = store
        .apply_edit(&chain, uid, LoopEdit::Trim { start: 64, end: 448 })
        .expect("a stopped loop can be trimmed");

    assert_eq!(new_len, 384 - SEAM_FRAMES);
    assert_eq!(store.status(&chain, uid).unwrap().len_frames, new_len);
    assert_eq!(
        store.export_raw(&chain, uid).unwrap().len() / 2,
        new_len,
        "the installed buffer is the edited one"
    );
}

#[test]
fn undo_restores_the_pre_edit_audio_sample_for_sample() {
    let (mut store, chain, uid) = store_with_recorded_loop(512);
    store.stop(&chain, uid);
    let before = store.export_raw(&chain, uid).unwrap();

    store.apply_edit(&chain, uid, LoopEdit::Cut { start: 100, end: 200 }).unwrap();
    assert_ne!(store.export_raw(&chain, uid).unwrap(), before);

    assert!(store.undo_edit(&chain, uid));
    assert_eq!(store.export_raw(&chain, uid).unwrap(), before);

    assert!(store.redo_edit(&chain, uid), "redo puts the edit back");
    assert_ne!(store.export_raw(&chain, uid).unwrap(), before);
}

#[test]
fn undo_on_an_untouched_loop_does_nothing() {
    let (mut store, chain, uid) = store_with_recorded_loop(512);
    store.stop(&chain, uid);
    assert!(!store.undo_edit(&chain, uid));
    assert!(!store.redo_edit(&chain, uid));
    assert_eq!(store.edit_history_depth(&chain, uid), (0, 0));
}

#[test]
fn the_history_is_capped_and_drops_the_oldest() {
    let (mut store, chain, uid) = store_with_recorded_loop(4096);
    store.stop(&chain, uid);
    for _ in 0..LOOPER_EDIT_HISTORY_MAX + 3 {
        store.apply_edit(&chain, uid, LoopEdit::Cut { start: 0, end: 100 }).unwrap();
    }
    assert_eq!(store.edit_history_depth(&chain, uid).0, LOOPER_EDIT_HISTORY_MAX);
}

#[test]
fn a_new_recording_clears_the_edit_history() {
    // The old buffers belong to audio that no longer exists; undoing into them
    // would resurrect a take the player replaced.
    let (mut store, chain, uid) = store_with_recorded_loop(512);
    store.stop(&chain, uid);
    store.apply_edit(&chain, uid, LoopEdit::Trim { start: 64, end: 448 }).unwrap();
    assert_eq!(store.edit_history_depth(&chain, uid).0, 1);

    store.clear(&chain, uid);
    assert_eq!(store.edit_history_depth(&chain, uid), (0, 0));
}
```

- [ ] **Step 2: Run the tests and watch them fail**

Run: `cargo test -p infra-cpal looper_store`
Expected: FAIL — `apply_edit` / `export_raw` / `undo_edit` do not exist.

- [ ] **Step 3: Implement**

In `crates/infra-cpal/src/looper_store.rs`:

1. Add to the per-slot entry struct:

```rust
    /// #826 — pre-edit buffers, newest last, and the redo tail. Control-thread
    /// only; the audio thread never sees these.
    edit_undo: Vec<Vec<f32>>,
    edit_redo: Vec<Vec<f32>>,
```

2. Add the constants and the refusal type near the top of the file:

```rust
/// #826: how many waveform edits can be undone. A 60 s stereo loop at 48 kHz
/// is ~23 MB, so the cap is memory, not taste — the oldest entry drops first.
pub const LOOPER_EDIT_HISTORY_MAX: usize = 8;

/// Why a waveform edit did not happen.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum LooperEditRefused {
    /// The looper is recording, overdubbing or playing.
    NotStopped,
    /// Nothing is recorded.
    Empty,
    /// No such chain/looper.
    Unknown,
    Edit(LoopEditError),
}
```

3. Add the methods:

```rust
    /// The recorded material without `mix`/`decay`/`reverse` — what the
    /// waveform editor draws and edits (#826).
    pub fn export_raw(&self, chain: &ChainId, uid: u64) -> Option<Vec<f32>> {
        self.slots
            .get(&(chain.clone(), uid))
            .and_then(|e| e.slot.export_raw())
    }

    /// Reshape a STOPPED loop (#826) and install the result, returning the new
    /// length in frames. The pre-edit buffer goes on the undo stack.
    pub fn apply_edit(
        &mut self,
        chain: &ChainId,
        uid: u64,
        edit: LoopEdit,
    ) -> Result<usize, LooperEditRefused> {
        let entry = self
            .slots
            .get(&(chain.clone(), uid))
            .ok_or(LooperEditRefused::Unknown)?;
        if entry.slot.state() != LooperState::Stopped {
            return Err(LooperEditRefused::NotStopped);
        }
        let before = entry.slot.export_raw().ok_or(LooperEditRefused::Empty)?;
        let edited = looper_edit::apply_edit(&before, edit).map_err(LooperEditRefused::Edit)?;

        self.load(chain, uid, &edited);
        if let Some(entry) = self.slots.get_mut(&(chain.clone(), uid)) {
            entry.edit_redo.clear();
            entry.edit_undo.push(before);
            if entry.edit_undo.len() > LOOPER_EDIT_HISTORY_MAX {
                entry.edit_undo.remove(0);
            }
        }
        Ok(edited.len() / 2)
    }

    /// Step back one waveform edit. `false` when there is nothing to undo.
    pub fn undo_edit(&mut self, chain: &ChainId, uid: u64) -> bool {
        self.step_edit_history(chain, uid, true)
    }

    /// Step forward one undone edit. `false` when there is nothing to redo.
    pub fn redo_edit(&mut self, chain: &ChainId, uid: u64) -> bool {
        self.step_edit_history(chain, uid, false)
    }

    fn step_edit_history(&mut self, chain: &ChainId, uid: u64, undo: bool) -> bool {
        let Some(entry) = self.slots.get_mut(&(chain.clone(), uid)) else {
            return false;
        };
        let from = if undo { &mut entry.edit_undo } else { &mut entry.edit_redo };
        let Some(target) = from.pop() else {
            return false;
        };
        let current = match entry.slot.export_raw() {
            Some(pcm) => pcm,
            None => return false,
        };
        if undo {
            entry.edit_redo.push(current);
        } else {
            entry.edit_undo.push(current);
        }
        self.load(chain, uid, &target);
        true
    }

    /// (undo depth, redo depth) — what the editor's buttons enable on (#826).
    pub fn edit_history_depth(&self, chain: &ChainId, uid: u64) -> (usize, usize) {
        self.slots
            .get(&(chain.clone(), uid))
            .map(|e| (e.edit_undo.len(), e.edit_redo.len()))
            .unwrap_or((0, 0))
    }

    fn clear_edit_history(&mut self, chain: &ChainId, uid: u64) {
        if let Some(e) = self.slots.get_mut(&(chain.clone(), uid)) {
            e.edit_undo.clear();
            e.edit_redo.clear();
        }
    }
```

4. Call `clear_edit_history` from `tap_record` (when a fresh recording starts), `clear` and `remove` — the buffers describe audio that no longer exists.

Note: `step_edit_history` borrows the entry then calls `self.load`; split the borrow (take the buffers out, drop the borrow, then `load`) so it compiles without `RefCell` gymnastics.

- [ ] **Step 4: Run the tests and verify they pass**

Run: `cargo test -p infra-cpal looper`
Expected: PASS, including the pre-existing store tests.

- [ ] **Step 5: Commit**

```bash
git add crates/infra-cpal/src/looper_store.rs crates/infra-cpal/src/looper_store_tests.rs
git commit -m "feat(#826): the looper store applies, undoes and redoes a waveform edit"
git push
```

---

### Task 5: Commands, events and the bus wiring

Every state change is a `Command` (the architecture law), so MCP and gRPC get the edits for free.

**Files:**
- Modify: `crates/application/src/command/looper.rs` (three variants)
- Modify: `crates/application/src/event.rs` (three events, near `ChainLooperParamChanged` ~line 447)
- Modify: `crates/application/src/local_dispatcher_looper.rs` (handler arms)
- Modify: `crates/infra-cpal/src/controller_loopers.rs` (controller passthrough, near `export_chain_looper` ~line 96)
- Modify: `crates/adapter-gui/src/looper_wiring.rs` (`apply_looper_event` arms + `looper_event_chain`)
- Test: `crates/application/src/local_dispatcher_looper_tests.rs`

**Interfaces:**
- Consumes: `LooperStore::{apply_edit, undo_edit, redo_edit, edit_history_depth}` (Task 4), `LoopEdit` (Task 3).
- Produces:
  - `LooperCommand::{EditChainLooperAudio { chain, looper, edit }, UndoChainLooperEdit { chain, looper }, RedoChainLooperEdit { chain, looper }}`
  - `Event::{ChainLooperEditApplied { chain, looper, edit }, ChainLooperEditUndone { chain, looper }, ChainLooperEditRedone { chain, looper }}`
  - `Controller::{looper_apply_edit(&self, chain, uid, edit) -> Result<usize, LooperEditRefused>, looper_undo_edit, looper_redo_edit, looper_edit_history_depth, export_chain_looper_raw}`

- [ ] **Step 1: Write the failing tests**

Append to `crates/application/src/local_dispatcher_looper_tests.rs` (follow the file's existing setup helper for a dispatcher with one chain and one looper):

```rust
#[test]
fn an_edit_command_emits_its_event() {
    let (dispatcher, chain, uid) = dispatcher_with_looper();
    let edit = LoopEdit::Trim { start: 10, end: 100 };

    let events = dispatcher
        .dispatch(Command::Looper(LooperCommand::EditChainLooperAudio {
            chain: chain.clone(),
            looper: uid,
            edit,
        }))
        .expect("a known looper accepts an edit");

    assert_eq!(
        events,
        vec![Event::ChainLooperEditApplied { chain, looper: uid, edit }],
        "the audio work happens in the wiring; the dispatcher's job is the event"
    );
}

#[test]
fn undo_and_redo_edit_commands_emit_their_events() {
    let (dispatcher, chain, uid) = dispatcher_with_looper();

    assert_eq!(
        dispatcher
            .dispatch(Command::Looper(LooperCommand::UndoChainLooperEdit {
                chain: chain.clone(),
                looper: uid,
            }))
            .unwrap(),
        vec![Event::ChainLooperEditUndone { chain: chain.clone(), looper: uid }]
    );
    assert_eq!(
        dispatcher
            .dispatch(Command::Looper(LooperCommand::RedoChainLooperEdit {
                chain: chain.clone(),
                looper: uid,
            }))
            .unwrap(),
        vec![Event::ChainLooperEditRedone { chain, looper: uid }]
    );
}

#[test]
fn editing_an_unknown_looper_is_an_error() {
    let (dispatcher, chain, _uid) = dispatcher_with_looper();
    assert!(dispatcher
        .dispatch(Command::Looper(LooperCommand::EditChainLooperAudio {
            chain,
            looper: 999,
            edit: LoopEdit::Trim { start: 0, end: 10 },
        }))
        .is_err());
}
```

- [ ] **Step 2: Run the tests and watch them fail**

Run: `cargo test -p application local_dispatcher_looper`
Expected: FAIL — the variants do not exist.

- [ ] **Step 3: Implement**

1. `crates/application/src/command/looper.rs` — add, with doc comments in the file's voice:

```rust
    /// #826: reshape a recorded loop — trim its bounds, crop to a region, or
    /// cut a region out. Frame indices over the loop as it stands. Applied on
    /// the control thread by the adapter wiring; refused unless the looper is
    /// stopped.
    EditChainLooperAudio {
        chain: ChainId,
        looper: u64,
        edit: LoopEdit,
    },

    /// #826: step back one waveform edit. Independent of the transport's
    /// undo (which is a no-op for the single-take looper).
    UndoChainLooperEdit { chain: ChainId, looper: u64 },

    /// #826: step forward one undone waveform edit.
    RedoChainLooperEdit { chain: ChainId, looper: u64 },
```

2. `crates/application/src/event.rs` — the three matching events, same field shapes.

3. `crates/application/src/local_dispatcher_looper.rs` — three arms, each resolving the looper with the existing `self.resolve_looper(&chain, looper)?` and returning the event. No project mutation (a recording is runtime state).

4. `crates/infra-cpal/src/controller_loopers.rs` — passthroughs to the store, mirroring `export_chain_looper`:

```rust
    /// #826: the loop's raw material, for the waveform editor.
    pub fn export_chain_looper_raw(&self, chain_id: &ChainId, uid: u64) -> Option<Vec<f32>> {
        self.looper_store.borrow().export_raw(chain_id, uid)
    }

    /// #826: reshape a stopped loop; the new length in frames on success.
    pub fn looper_apply_edit(
        &self,
        chain_id: &ChainId,
        uid: u64,
        edit: LoopEdit,
    ) -> Result<usize, LooperEditRefused> {
        self.looper_store.borrow_mut().apply_edit(chain_id, uid, edit)
    }
```

plus `looper_undo_edit`, `looper_redo_edit`, `looper_edit_history_depth`.

5. `crates/adapter-gui/src/looper_wiring.rs` — new arms in `apply_looper_event`, and add the three events to `looper_event_chain` so the isolated playback stream is reconciled after an edit (the loop's length changed):

```rust
        Event::ChainLooperEditApplied { chain, looper, edit } => {
            if let Err(err) = controller.looper_apply_edit(chain, *looper, *edit) {
                log::warn!("editing loop {looper} of chain {}: {err:?}", chain.0);
            }
        }
        Event::ChainLooperEditUndone { chain, looper } => {
            controller.looper_undo_edit(chain, *looper);
        }
        Event::ChainLooperEditRedone { chain, looper } => {
            controller.looper_redo_edit(chain, *looper);
        }
```

- [ ] **Step 4: Run the tests and verify they pass**

Run: `cargo test -p application local_dispatcher_looper && cargo build -p adapter-gui`
Expected: PASS, clean build (the MCP tool list picks the variants up automatically — confirm `cargo test -p adapter-mcp` still passes).

- [ ] **Step 5: Commit**

```bash
git add crates/application/src/command/looper.rs crates/application/src/event.rs crates/application/src/local_dispatcher_looper.rs crates/application/src/local_dispatcher_looper_tests.rs crates/infra-cpal/src/controller_loopers.rs crates/adapter-gui/src/looper_wiring.rs
git commit -m "feat(#826): loop edits travel the command bus"
git push
```

---

### Task 6: The waveform editor overlay (visual)

**Files:**
- Create: `crates/adapter-gui/ui/components/waveform_view.slint`
- Create: `crates/adapter-gui/ui/components/looper_editor_overlay.slint`
- Create: `crates/adapter-gui/ui/components/looper_editor_test_harness.slint`
- Modify: `crates/adapter-gui/ui/components/looper_panel_globals.slint` (the editor's global state)
- Modify: `crates/adapter-gui/ui/app-window.slint` (mount the overlay at the root, next to the existing looper overlay; declare the peaks property and the callbacks)

**Interfaces:**
- Consumes: nothing from Rust yet (Task 7 wires it).
- Produces:
  - `global LooperEditor { open: bool, chain-index: int, uid: int, peaks: [float], sel-start: float, sel-end: float, can-undo: bool, can-redo: bool }` (ratios 0..1 for the selection)
  - `AppWindow` callbacks `looper-edit-apply(int /*chain*/, int /*uid*/, int /*kind: 0 trim, 1 crop, 2 cut*/, float /*start*/, float /*end*/)`, `looper-edit-undo(int, int)`, `looper-edit-redo(int, int)`
  - `component WaveformView { in property <[float]> peaks; in property <float> sel-start; in property <float> sel-end; }`

- [ ] **Step 1: Read the UI rules first**

Invoke the `slint-best-practices` skill and read `crates/adapter-gui/ui/components/looper_overlay.slint` (the shape this copies: root overlay, backdrop `TouchArea`, callbacks bubbling with the chain index). Icons are SVG via `@image-url` + colorize — never a glyph.

- [ ] **Step 2: Build the waveform component**

`waveform_view.slint`: a horizontal row of `WAVEFORM_BUCKETS` bars from `peaks` (`for p[i] in peaks`), each a `Rectangle` centred vertically with `height: max(1px, p * parent.height)`, `Theme` colours, and the region outside `[sel-start, sel-end]` dimmed by an overlay `Rectangle` on each side. Two handle `Rectangle`s with `TouchArea { moved => ... }` update `LooperEditor.sel-start` / `sel-end`, clamped to `0..1` and to each other.

- [ ] **Step 3: Build the overlay**

`looper_editor_overlay.slint`: full-window `Rectangle`, `visible: LooperEditor.open`, backdrop `TouchArea` that closes it, a centred card with the title, the `WaveformView`, and a button row — `Trim`, `Crop`, `Cut` (disabled while `sel-start >= sel-end`), `Undo` (`enabled: LooperEditor.can-undo`), `Redo` (`enabled: LooperEditor.can-redo`), `Close`. Every string uses `@tr(...)`. Buttons call the `AppWindow` callbacks with `LooperEditor.chain-index`, `LooperEditor.uid` and the selection ratios.

- [ ] **Step 4: Render and LOOK at the PNG**

```bash
cargo run -p slint-render -- crates/adapter-gui/ui/components/looper_editor_test_harness.slint /tmp/826-editor.png
```

(the harness seeds `LooperEditor.open = true` and a synthetic `peaks` array — copy the shape of `looper_panel_test_harness.slint`).
Open the PNG and check it: bars visible, selection dimming on both sides, handles on the bounds, buttons legible, nothing clipped. Iterate until it looks right — do NOT claim it works from the code alone.

- [ ] **Step 5: Commit**

```bash
git add crates/adapter-gui/ui/components/waveform_view.slint crates/adapter-gui/ui/components/looper_editor_overlay.slint crates/adapter-gui/ui/components/looper_editor_test_harness.slint crates/adapter-gui/ui/components/looper_panel_globals.slint crates/adapter-gui/ui/app-window.slint
git commit -m "feat(#826): waveform editor overlay"
git push
```

---

### Task 7: Wire the editor to the bus

**Files:**
- Modify: `crates/adapter-gui/ui/models.slint` (`LooperItem.can_edit`)
- Modify: `crates/adapter-gui/ui/components/looper_row.slint` (an edit button next to `clear-btn`, `enabled: can_edit`)
- Modify: `crates/adapter-gui/src/looper_view.rs` (fill `can_edit`)
- Modify: `crates/adapter-gui/src/looper_callbacks.rs` (open the editor, feed the peaks, dispatch the edits)
- Create: `crates/adapter-gui/tests/issue_826_looper_editor_interaction.rs`
- Test: `crates/adapter-gui/src/looper_view_tests.rs`

**Interfaces:**
- Consumes: the commands of Task 5, `Controller::export_chain_looper_raw`, `application::looper_edit::peaks`, the Slint globals of Task 6.
- Produces: nothing downstream.

- [ ] **Step 1: Write the failing tests**

In `crates/adapter-gui/src/looper_view_tests.rs`:

```rust
#[test]
fn can_edit_only_when_the_loop_is_stopped_and_has_material() {
    // The edit button must be dead while recording/playing and on an empty
    // looper — the store refuses those, so an enabled button would be a lie.
    let stopped = looper_items(/* … a stopped looper with len_frames > 0 … */);
    assert!(stopped[0].can_edit);

    let playing = looper_items(/* … the same looper, Playing … */);
    assert!(!playing[0].can_edit);

    let empty = looper_items(/* … Empty … */);
    assert!(!empty[0].can_edit);
}
```

(Follow the file's existing helpers for building statuses — `looper_items_with_recorded` takes the statuses directly.)

In `crates/adapter-gui/tests/issue_826_looper_editor_interaction.rs`, an `i-slint-backend-testing` test in the shape of `tests/issue_323_looper_panel_interaction.rs`: open the editor, drag the start handle, click `Trim`, and assert the dispatched command is `EditChainLooperAudio` with frame bounds matching the dragged ratios. Rendering alone proves nothing about clicks (#749/#761).

- [ ] **Step 2: Run the tests and watch them fail**

Run: `cargo test -p adapter-gui looper_view && cargo test -p adapter-gui --test issue_826_looper_editor_interaction`
Expected: FAIL — `can_edit` missing, no editor wiring.

- [ ] **Step 3: Implement**

1. `models.slint`: `/// #826: whether the waveform editor may open — stopped and non-empty.` `can_edit: bool`.
2. `looper_view.rs`: `can_edit: status.state == LooperState::Stopped && status.len_frames > 0`.
3. `looper_row.slint`: an edit button (`assets/`-hosted SVG, `@tr("looper-edit")` label) that emits `edit()`; `looper_panel.slint` / `looper_overlay.slint` bubble it up with the chain index, exactly as `clear` does.
4. `looper_callbacks.rs`:
   - `on_looper_edit(index, uid)`: read `controller.export_chain_looper_raw`, compute `looper_edit::peaks(&pcm, WAVEFORM_BUCKETS)`, push it into `LooperEditor.peaks`, set `chain-index`/`uid`/`sel-start = 0.0`/`sel-end = 1.0`/`open = true`, and refresh `can-undo`/`can-redo` from `controller.looper_edit_history_depth`.
   - `on_looper_edit_apply(index, uid, kind, start, end)`: convert the ratios to frames with the loop's current length (`status.len_frames`), build the `LoopEdit` variant for `kind`, `dispatch_and_apply` it, then re-read the peaks and the history depths so the view follows, and mark the project dirty.
   - `on_looper_edit_undo` / `on_looper_edit_redo`: dispatch, then refresh peaks + depths the same way.
   - Refuse nothing in the callback — the store is the single gate; log its refusal.

- [ ] **Step 4: Run the tests and verify they pass**

Run: `cargo test -p adapter-gui`
Expected: PASS, including the interaction test.

- [ ] **Step 5: Commit**

```bash
git add crates/adapter-gui/ui/models.slint crates/adapter-gui/ui/components/looper_row.slint crates/adapter-gui/ui/components/looper_panel.slint crates/adapter-gui/ui/components/looper_overlay.slint crates/adapter-gui/src/looper_view.rs crates/adapter-gui/src/looper_view_tests.rs crates/adapter-gui/src/looper_callbacks.rs crates/adapter-gui/tests/issue_826_looper_editor_interaction.rs
git commit -m "feat(#826): the looper row opens the waveform editor"
git push
```

---

### Task 8: Translations, docs and the full suite

**Files:**
- Modify: `crates/adapter-gui/translations/*/LC_MESSAGES/adapter-gui.po` (all 9 locales)
- Modify: `docs/screens.md` (the editor overlay), `docs/testing.md` (the new test files, if the file lists them)
- Modify: `README.md`, `README.pt-BR.md`, `README.es-ES.md` only if the looper's feature list is enumerated there

- [ ] **Step 1: Extract the new strings**

Run: `scripts/extract-translations.sh`
Then fill every new msgid in all 9 catalogs — English is the reference; no empty msgstr left behind.

- [ ] **Step 2: Document the feature**

Add a `docs/screens.md` subsection under the looper: what the editor shows, that it is stopped-only, that trim/crop/cut apply to the whole loop (the layer history is consumed), that edits have their own undo capped at 8, and that the edited loop is written to disk on the next project save.

- [ ] **Step 3: Run the full suite and the linter**

```bash
cargo build --workspace
cargo clippy --workspace --all-targets -- -D warnings
cargo test --workspace
```
Expected: clean build, no warnings, all green. Paste the summary line into the issue comment.

- [ ] **Step 4: Commit**

```bash
git add crates/adapter-gui/translations docs/screens.md docs/testing.md
git commit -m "docs(#826): document the loop waveform editor"
git push
```

- [ ] **Step 5: Report and hand off**

`gh issue comment 826` with the commit hashes, the files touched, the test result — and the validation checklist (the `git fetch && git checkout feature/issue-826 && git pull` command plus numbered `- [ ]` items covering: the edit button is dead while playing, trim tightens the loop, crop keeps the selection, cut closes the gap with no click at the wrap, undo brings the take back, and the edited loop survives save/reopen).

---

## Self-Review

- **Spec coverage:** raw export → Task 1; seam curve → Task 2; trim/crop/cut + peaks + errors → Task 3; stopped-only gate, history cap, history clearing → Task 4; commands/events/wiring/MCP parity → Task 5; overlay + waveform + no PopupWindow → Task 6; row button, peaks feed, interaction proof → Task 7; translations, docs, full suite → Task 8. The spec's "wav rewritten on save, not inline" is covered by Task 7 (mark dirty) plus the existing `save_chain_loops`.
- **Type consistency:** `LoopEdit` / `LoopEditError` / `SEAM_FRAMES` / `MIN_LOOP_FRAMES` (Task 3) are used unchanged in Tasks 4, 5 and 7; `LooperEditRefused` and `LOOPER_EDIT_HISTORY_MAX` (Task 4) are used unchanged in Task 5; `export_raw` (Task 1) is used in Tasks 4 and 5.
- **Not in scope, deliberately:** audio preview of a pending edit, BPM/grid snap, fade handles, per-layer editing.
