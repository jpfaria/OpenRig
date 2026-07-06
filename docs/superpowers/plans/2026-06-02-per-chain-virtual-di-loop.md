# Per-chain virtual DI loop — Implementation Plan (issue #614)

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.
>
> **OpenRig red-first gate:** every task starts by writing a test and watching it FAIL. Only after seeing RED do you touch production code (the `dev-rules` plugin hook enforces this; create `.dev-rules/.red-first-unlocked` after showing the RED run, per `docs/testing.md`). Push after each commit; `gh issue comment` after each push. The shared quality gate runs only at PR creation — never run it on push.

**Goal:** Let the user shape tone hands-free by injecting a looping dry guitar DI in place of a single chain's live input, ephemerally, passing through that chain's full block graph.

**Architecture:** A new immutable `DiLoop` buffer (engine) is published per `ChainRuntimeState` via `ArcSwapOption` + an `AtomicUsize` cursor. On the audio thread, `process_input_f32` reads the loop instead of the device frame when set — lock-free, zero-alloc, mirroring the existing latency-probe injection. Decode→resample→loop-crossfade happens off-thread in the command side-effect. Two transport-agnostic Commands drive it; the Chains UI adds an icon next to the per-chain volume (select + play/stop).

**Tech Stack:** Rust (engine/application/adapter-gui), Slint UI, `arc_swap`, existing WAV decode in `crates/adapter-render/src/wav.rs`.

---

## File Structure

- Create `crates/engine/src/di_loop.rs` — `DiFrame`, `DiLoop`, pure `from_samples` (resample + equal-power loop crossfade). Test: `crates/engine/src/di_loop_tests.rs`.
- Modify `crates/engine/src/runtime_state.rs` — add `di_loop: ArcSwapOption<DiLoop>` + `di_loop_pos: AtomicUsize` fields + `set_di_loop` / `has_di_loop` methods on `ChainRuntimeState`.
- Modify `crates/engine/src/runtime_graph.rs` — initialize the two new fields in `build_chain_runtime_state`.
- Modify `crates/engine/src/runtime.rs` — DI branch in `process_input_f32` + `process_single_segment` (new `di` param). Test: `crates/engine/src/di_loop_injection_tests.rs`.
- Modify `crates/engine/src/lib.rs` — register the `di_loop` module + re-export `DiLoop`, `DiFrame`.
- Modify `crates/application/src/command.rs` — `SetChainDiLoopSource` + `SetChainDiLoopEnabled` variants + dispatch. Tests in the command test module used by that crate.
- Create the DI source preload glue in the side-effect layer (application) that decodes the file and calls `DiLoop::from_samples`.
- Modify `crates/adapter-gui/**` — event → pure function → command wiring for the chain-tile control. Test: a new `adapter-gui/tests/issue_614_*.rs` for the pure event handler.
- Modify the Chains screen Slint (the chain tile that hosts the volume control) — icon button + select + play/stop, dispatching callbacks.
- Create `assets/di-loops/` — 1-2 CC0 DI loops (license-approved before commit) + a small index.
- Modify `docs/screens.md` (Chains) and `docs/audio-config.md` if relevant.

---

## Task 1: `DiLoop` type + pure preload (resample + loop crossfade)

**Files:**
- Create: `crates/engine/src/di_loop.rs`
- Create: `crates/engine/src/di_loop_tests.rs`
- Modify: `crates/engine/src/lib.rs` (register module + re-export)

- [ ] **Step 1: Write the failing tests**

Create `crates/engine/src/di_loop_tests.rs`:

```rust
use super::*;
use block_core::AudioChannelLayout;

#[test]
fn from_samples_mono_no_resample_preserves_len_and_layout() {
    // 1 channel, src_sr == engine_sr → no resample, no crossfade region change in length.
    let samples = vec![0.0, 0.1, 0.2, 0.3, 0.4, 0.5, 0.6, 0.7];
    let di = DiLoop::from_samples(&samples, 48_000, 1, 48_000, 0);
    assert_eq!(di.layout(), AudioChannelLayout::Mono);
    assert_eq!(di.len(), 8);
}

#[test]
fn from_samples_stereo_deinterleaves_to_stereo_frames() {
    // interleaved L,R,L,R → 2 stereo frames
    let samples = vec![0.1, 0.2, 0.3, 0.4];
    let di = DiLoop::from_samples(&samples, 48_000, 2, 48_000, 0);
    assert_eq!(di.layout(), AudioChannelLayout::Stereo);
    assert_eq!(di.len(), 2);
    match di.frame_at(0) {
        DiFrame::Stereo([l, r]) => assert!((l - 0.1).abs() < 1e-6 && (r - 0.2).abs() < 1e-6),
        _ => panic!("expected stereo"),
    }
}

#[test]
fn frame_at_wraps_around() {
    let samples = vec![0.0, 1.0, 2.0];
    let di = DiLoop::from_samples(&samples, 48_000, 1, 48_000, 0);
    match (di.frame_at(0), di.frame_at(3), di.frame_at(4)) {
        (DiFrame::Mono(a), DiFrame::Mono(b), DiFrame::Mono(c)) => {
            assert_eq!(a, 0.0);
            assert_eq!(b, 0.0); // wrapped
            assert_eq!(c, 1.0); // wrapped
        }
        _ => panic!("mono expected"),
    }
}

#[test]
fn resample_doubles_length_when_target_is_double() {
    // src 24k → engine 48k doubles frame count (±1).
    let samples = vec![0.0, 0.25, 0.5, 0.75];
    let di = DiLoop::from_samples(&samples, 24_000, 1, 48_000, 0);
    assert!((di.len() as i64 - 8).abs() <= 1, "len was {}", di.len());
}

#[test]
fn loop_crossfade_makes_seam_continuous() {
    // A ramp 0..1 has a hard jump at the wrap (1.0 → 0.0). With a crossfade
    // window the last sample is pulled toward the first so the wrap step is
    // smaller than the un-crossfaded step.
    let n = 256;
    let samples: Vec<f32> = (0..n).map(|i| i as f32 / n as f32).collect();
    let xfade = 32;
    let di = DiLoop::from_samples(&samples, 48_000, 1, 48_000, xfade);
    let last = match di.frame_at(di.len() - 1) { DiFrame::Mono(s) => s, _ => unreachable!() };
    let first = match di.frame_at(0) { DiFrame::Mono(s) => s, _ => unreachable!() };
    let seam_step = (first - last).abs();
    assert!(seam_step < 0.5, "seam step {seam_step} not reduced by crossfade");
}
```

- [ ] **Step 2: Run the tests to verify they fail**

Run: `cargo test -p engine di_loop`
Expected: FAIL — `DiLoop` / `DiFrame` not found. (Create `.dev-rules/.red-first-unlocked` after showing this RED run.)

- [ ] **Step 3: Implement `di_loop.rs`**

Create `crates/engine/src/di_loop.rs`:

```rust
//! Ephemeral per-chain virtual DI loop source (issue #614).
//!
//! An immutable, preallocated, read-only buffer of mono-or-stereo frames at
//! the engine sample rate. Built entirely OFF the audio thread (decode +
//! resample + loop crossfade); read lock-free ON the audio thread via
//! [`DiLoop::frame_at`]. Nothing in this module allocates or locks once the
//! buffer is built, so it is safe to read from `process_input_f32`.

use block_core::AudioChannelLayout;

/// One frame of the DI loop, in the loop's own layout.
#[derive(Clone, Copy, Debug, PartialEq)]
pub enum DiFrame {
    Mono(f32),
    Stereo([f32; 2]),
}

/// A looping dry-DI buffer at the engine sample rate.
pub struct DiLoop {
    frames: Box<[DiFrame]>,
    layout: AudioChannelLayout,
}

impl DiLoop {
    /// Build a loop from interleaved `samples` (`channels` interleaved) at
    /// `src_sr`, resampled to `engine_sr`, with an equal-power loop crossfade
    /// of `xfade_frames` at the wrap seam (0 = no crossfade).
    ///
    /// `channels >= 2` keeps channels 0 and 1 as stereo; `channels == 1` is
    /// mono. Runs off the audio thread.
    pub fn from_samples(
        samples: &[f32],
        src_sr: u32,
        channels: usize,
        engine_sr: u32,
        xfade_frames: usize,
    ) -> Self {
        let layout = if channels >= 2 {
            AudioChannelLayout::Stereo
        } else {
            AudioChannelLayout::Mono
        };
        let ch = channels.max(1);

        // 1. De-interleave into per-frame samples.
        let src_frames: Vec<DiFrame> = samples
            .chunks(ch)
            .map(|c| match layout {
                AudioChannelLayout::Stereo => {
                    DiFrame::Stereo([*c.first().unwrap_or(&0.0), *c.get(1).unwrap_or(&0.0)])
                }
                AudioChannelLayout::Mono => DiFrame::Mono(*c.first().unwrap_or(&0.0)),
            })
            .collect();

        // 2. Resample to engine_sr (linear interpolation; identity when equal).
        let mut frames = resample_frames(&src_frames, src_sr, engine_sr, layout);

        // 3. Equal-power loop crossfade at the seam.
        apply_loop_crossfade(&mut frames, xfade_frames, layout);

        Self {
            frames: frames.into_boxed_slice(),
            layout,
        }
    }

    #[inline(always)]
    pub fn layout(&self) -> AudioChannelLayout {
        self.layout
    }

    #[inline(always)]
    pub fn len(&self) -> usize {
        self.frames.len()
    }

    #[inline(always)]
    pub fn is_empty(&self) -> bool {
        self.frames.is_empty()
    }

    /// Frame at `pos`, wrapping modulo length. Silence if empty.
    #[inline(always)]
    pub fn frame_at(&self, pos: usize) -> DiFrame {
        if self.frames.is_empty() {
            return match self.layout {
                AudioChannelLayout::Stereo => DiFrame::Stereo([0.0, 0.0]),
                AudioChannelLayout::Mono => DiFrame::Mono(0.0),
            };
        }
        self.frames[pos % self.frames.len()]
    }
}

/// Linear-interpolation resample. Identity (clone) when `src_sr == dst_sr`.
/// Linear is adequate for a practice DI loop; a windowed-sinc upgrade is
/// tracked as a follow-up. Runs off the audio thread.
fn resample_frames(
    src: &[DiFrame],
    src_sr: u32,
    dst_sr: u32,
    layout: AudioChannelLayout,
) -> Vec<DiFrame> {
    if src_sr == dst_sr || src.len() < 2 {
        return src.to_vec();
    }
    let ratio = dst_sr as f64 / src_sr as f64;
    let out_len = ((src.len() as f64) * ratio).round() as usize;
    let mut out = Vec::with_capacity(out_len);
    for i in 0..out_len {
        let src_pos = i as f64 / ratio;
        let idx = src_pos.floor() as usize;
        let frac = (src_pos - idx as f64) as f32;
        let a = src[idx.min(src.len() - 1)];
        let b = src[(idx + 1).min(src.len() - 1)];
        out.push(lerp_frame(a, b, frac, layout));
    }
    out
}

#[inline]
fn lerp_frame(a: DiFrame, b: DiFrame, t: f32, layout: AudioChannelLayout) -> DiFrame {
    match layout {
        AudioChannelLayout::Mono => {
            let av = if let DiFrame::Mono(s) = a { s } else { 0.0 };
            let bv = if let DiFrame::Mono(s) = b { s } else { 0.0 };
            DiFrame::Mono(av + (bv - av) * t)
        }
        AudioChannelLayout::Stereo => {
            let [al, ar] = if let DiFrame::Stereo(v) = a { v } else { [0.0, 0.0] };
            let [bl, br] = if let DiFrame::Stereo(v) = b { v } else { [0.0, 0.0] };
            DiFrame::Stereo([al + (bl - al) * t, ar + (br - ar) * t])
        }
    }
}

/// Equal-power crossfade of the loop's tail into its head over `xfade` frames,
/// so the wrap from last→first sample has no click. No-op if `xfade == 0` or
/// the buffer is too short.
fn apply_loop_crossfade(frames: &mut [DiFrame], xfade: usize, layout: AudioChannelLayout) {
    let n = frames.len();
    if xfade == 0 || n < xfade * 2 {
        return;
    }
    for i in 0..xfade {
        // progress 0..1 across the tail window
        let p = (i + 1) as f32 / (xfade + 1) as f32;
        // equal-power: tail fades out, head (the symmetric frame) fades in
        let tail_g = (0.5 * std::f32::consts::PI * p).cos();
        let head_g = (0.5 * std::f32::consts::PI * p).sin();
        let tail_idx = n - xfade + i;
        let head_idx = i;
        let tail = frames[tail_idx];
        let head = frames[head_idx];
        frames[tail_idx] = mix_frame(tail, tail_g, head, head_g, layout);
    }
}

#[inline]
fn mix_frame(a: DiFrame, ga: f32, b: DiFrame, gb: f32, layout: AudioChannelLayout) -> DiFrame {
    match layout {
        AudioChannelLayout::Mono => {
            let av = if let DiFrame::Mono(s) = a { s } else { 0.0 };
            let bv = if let DiFrame::Mono(s) = b { s } else { 0.0 };
            DiFrame::Mono(av * ga + bv * gb)
        }
        AudioChannelLayout::Stereo => {
            let [al, ar] = if let DiFrame::Stereo(v) = a { v } else { [0.0, 0.0] };
            let [bl, br] = if let DiFrame::Stereo(v) = b { v } else { [0.0, 0.0] };
            DiFrame::Stereo([al * ga + bl * gb, ar * ga + br * gb])
        }
    }
}

#[cfg(test)]
#[path = "di_loop_tests.rs"]
mod tests;
```

Register in `crates/engine/src/lib.rs` (add near the other `mod`/`pub use` lines):

```rust
mod di_loop;
pub use di_loop::{DiFrame, DiLoop};
```

- [ ] **Step 4: Run the tests to verify they pass**

Run: `cargo test -p engine di_loop`
Expected: PASS (5 tests).

- [ ] **Step 5: Commit**

```bash
git add crates/engine/src/di_loop.rs crates/engine/src/di_loop_tests.rs crates/engine/src/lib.rs
git commit -m "feat(#614): DiLoop type with off-thread resample + loop crossfade"
git push
```
Then `gh issue comment 614` with hash + files + test result.

---

## Task 2: `ChainRuntimeState` DI fields + accessors

**Files:**
- Modify: `crates/engine/src/runtime_state.rs` (struct fields ~252-354, impl ~356-587)
- Modify: `crates/engine/src/runtime_graph.rs` (`build_chain_runtime_state` struct literal)
- Test: add to `crates/engine/src/runtime_tests.rs` (or a new `di_loop_state_tests.rs`)

- [ ] **Step 1: Write the failing test**

Add `crates/engine/src/di_loop_state_tests.rs`:

```rust
use super::*;
use crate::di_loop::DiLoop;
use std::sync::Arc;

#[test]
fn set_di_loop_publishes_and_resets_cursor() {
    let runtime = crate::runtime_graph::build_chain_runtime_state_for_test();
    assert!(!runtime.has_di_loop());
    runtime
        .di_loop_pos
        .store(123, std::sync::atomic::Ordering::Relaxed);
    let di = Arc::new(DiLoop::from_samples(&[0.0, 0.5, 1.0], 48_000, 1, 48_000, 0));
    runtime.set_di_loop(Some(di));
    assert!(runtime.has_di_loop());
    assert_eq!(
        runtime.di_loop_pos.load(std::sync::atomic::Ordering::Relaxed),
        0
    );
    runtime.set_di_loop(None);
    assert!(!runtime.has_di_loop());
}
```

> NOTE at execution: if no `build_chain_runtime_state_for_test()` helper exists, construct the runtime via the existing test harness used elsewhere in `runtime_tests.rs` (mirror how those tests build a `ChainRuntimeState`). Wire this module with `#[cfg(test)] #[path = "di_loop_state_tests.rs"] mod di_loop_state;` in `runtime.rs`.

- [ ] **Step 2: Run it to verify it fails**

Run: `cargo test -p engine di_loop_state`
Expected: FAIL — `has_di_loop` / `set_di_loop` / `di_loop_pos` not found.

- [ ] **Step 3: Implement the fields + accessors**

In `crates/engine/src/runtime_state.rs`, add the import:

```rust
use arc_swap::{ArcSwap, ArcSwapOption};
use std::sync::atomic::{ /* existing */ AtomicUsize, Ordering};
use crate::di_loop::DiLoop;
```

Add to the `ChainRuntimeState` struct (after `bypass_block_ids`):

```rust
    /// Ephemeral per-chain virtual DI loop (issue #614). `None` ⇒ the chain
    /// reads its live device input as usual. Published off-thread via
    /// `ArcSwapOption`; read lock-free on the audio thread in
    /// `process_input_f32`. Never persisted to the project (tone-shaping aid).
    pub(crate) di_loop: ArcSwapOption<DiLoop>,
    /// Playback cursor (frame index into the DI loop). Advanced once per input
    /// callback by `process_input_f32`. Relaxed: only this single audio thread
    /// writes it, and the value is purely a playback position.
    pub(crate) di_loop_pos: AtomicUsize,
```

Add methods in `impl ChainRuntimeState`:

```rust
    /// Publish (or clear) the chain's virtual DI loop and reset the cursor.
    /// Called off the audio thread by the command side-effect.
    pub fn set_di_loop(&self, di: Option<Arc<DiLoop>>) {
        self.di_loop.store(di);
        self.di_loop_pos.store(0, Ordering::Relaxed);
    }

    /// Whether a DI loop is currently active for this chain.
    pub fn has_di_loop(&self) -> bool {
        self.di_loop.load().is_some()
    }
```

In `crates/engine/src/runtime_graph.rs`, in `build_chain_runtime_state`'s `ChainRuntimeState { ... }` literal, initialize:

```rust
            di_loop: arc_swap::ArcSwapOption::empty(),
            di_loop_pos: std::sync::atomic::AtomicUsize::new(0),
```

- [ ] **Step 4: Run it to verify it passes**

Run: `cargo test -p engine di_loop_state`
Expected: PASS.

- [ ] **Step 5: Commit**

```bash
git add crates/engine/src/runtime_state.rs crates/engine/src/runtime_graph.rs crates/engine/src/runtime.rs crates/engine/src/di_loop_state_tests.rs
git commit -m "feat(#614): per-chain DI loop fields + accessors on ChainRuntimeState"
git push
```
Then `gh issue comment 614`.

---

## Task 3: Audio-thread injection in `process_input_f32`

**Files:**
- Modify: `crates/engine/src/runtime.rs` (`process_input_f32` ~86-235; `process_single_segment` ~299-459)
- Test: `crates/engine/src/di_loop_injection_tests.rs`

- [ ] **Step 1: Write the failing tests**

Create `crates/engine/src/di_loop_injection_tests.rs`. These drive a built runtime, feed device input that is all-zeros, set a DI loop with known samples, run `process_input_f32` + `process_output_f32`, and assert the OUTPUT carries the DI (not the zero device input). Mirror the harness used by `audio_signal_integrity_tests.rs` for building a passthrough chain and pumping callbacks.

```rust
use super::*;
use crate::di_loop::DiLoop;
use std::sync::Arc;

// Build a single-block passthrough chain runtime (stereo) using the same
// helper the other audio tests use; see audio_signal_integrity_tests.rs.
fn passthrough_runtime() -> Arc<ChainRuntimeState> {
    // EXECUTION: reuse the existing builder used by audio_signal_integrity_tests
    // (e.g. a gain=unity chain). Placeholder name shown; wire to the real one.
    crate::runtime_graph::build_passthrough_runtime_for_test(2 /* channels */)
}

#[test]
fn di_loop_replaces_silent_device_input() {
    let runtime = passthrough_runtime();
    let di = Arc::new(DiLoop::from_samples(&[0.5; 256], 48_000, 1, 48_000, 0));
    runtime.set_di_loop(Some(di));

    let frames = 128usize;
    let channels = 2usize;
    let device_in = vec![0.0f32; frames * channels]; // silence from device
    process_input_f32(&runtime, 0, &device_in, channels);

    let mut out = vec![0.0f32; frames * channels];
    process_output_f32(&runtime, 0, &mut out, channels);

    // DI was 0.5 mono → broadcast stereo → audible at output despite silent device.
    let peak = out.iter().cloned().fold(0.0f32, |m, s| m.max(s.abs()));
    assert!(peak > 0.1, "DI loop did not reach output (peak {peak})");
}

#[test]
fn off_is_silent_passthrough_of_device() {
    let runtime = passthrough_runtime();
    // no DI set
    let frames = 128usize;
    let channels = 2usize;
    let device_in = vec![0.0f32; frames * channels];
    process_input_f32(&runtime, 0, &device_in, channels);
    let mut out = vec![0.0f32; frames * channels];
    process_output_f32(&runtime, 0, &mut out, channels);
    let peak = out.iter().cloned().fold(0.0f32, |m, s| m.max(s.abs()));
    assert!(peak < 1e-6, "expected silence with no DI, got peak {peak}");
}

#[test]
fn cursor_advances_by_num_frames_and_wraps() {
    let runtime = passthrough_runtime();
    let di = Arc::new(DiLoop::from_samples(&vec![0.1f32; 200], 48_000, 1, 48_000, 0));
    runtime.set_di_loop(Some(di));
    let frames = 128usize;
    let channels = 2usize;
    let device_in = vec![0.0f32; frames * channels];
    process_input_f32(&runtime, 0, &device_in, channels);
    assert_eq!(
        runtime.di_loop_pos.load(std::sync::atomic::Ordering::Relaxed),
        128
    );
    process_input_f32(&runtime, 0, &device_in, channels);
    // 256 % 200 == 56
    assert_eq!(
        runtime.di_loop_pos.load(std::sync::atomic::Ordering::Relaxed),
        56
    );
}
```

Wire with `#[cfg(test)] #[path = "di_loop_injection_tests.rs"] mod di_loop_injection;` in `runtime.rs`.

- [ ] **Step 2: Run to verify it fails**

Run: `cargo test -p engine di_loop_injection`
Expected: FAIL — output is silent because injection isn't implemented (and helper to wire).

- [ ] **Step 3: Implement the injection**

In `process_input_f32`, AFTER the probe `data` rebinding (~line 149) and BEFORE the input-taps block, add:

```rust
    // ── Virtual DI loop (issue #614) ─────────────────────────────────────
    // If a DI loop is published for this chain, every segment reads the loop
    // instead of the device frame. Lock-free (ArcSwapOption load) and
    // zero-alloc (we pass a borrow + a shared start cursor into the segments;
    // the cursor advances once per callback below). `None` ⇒ one branch, then
    // identical to today's device path. Input taps below intentionally keep
    // reading the device `data` (the tuner tracks the real input).
    let di_guard = runtime.di_loop.load();
    let di_ref: Option<&crate::di_loop::DiLoop> = di_guard.as_deref();
    let di_start = match di_ref {
        Some(_) => runtime.di_loop_pos.load(Ordering::Relaxed),
        None => 0,
    };
    let di_for_seg = di_ref.map(|d| (d, di_start));
```

Change `process_single_segment`'s signature to accept the DI:

```rust
fn process_single_segment(
    input_states: &mut [InputProcessingState],
    scratch: &mut InputCallbackScratch,
    seg_idx: usize,
    data: &[f32],
    input_total_channels: usize,
    num_frames: usize,
    error_queue: &ArrayQueue<BlockError>,
    stream_taps: &[Arc<StreamTap>],
    di: Option<(&crate::di_loop::DiLoop, usize)>,
) {
```

Replace the device-read loop (current lines ~331-344) with:

```rust
    match di {
        Some((di_loop, start_pos)) => {
            use crate::di_loop::DiFrame;
            for i in 0..num_frames {
                let f = di_loop.frame_at(start_pos.wrapping_add(i));
                let chain_frame = match (*processing_layout, f) {
                    (AudioChannelLayout::Stereo, DiFrame::Mono(s)) => AudioFrame::Stereo([s, s]),
                    (AudioChannelLayout::Stereo, DiFrame::Stereo(lr)) => AudioFrame::Stereo(lr),
                    (AudioChannelLayout::Mono, DiFrame::Mono(s)) => AudioFrame::Mono(s),
                    (AudioChannelLayout::Mono, DiFrame::Stereo([l, r])) => {
                        AudioFrame::Mono((l + r) * 0.5)
                    }
                };
                frame_buffer.push(chain_frame);
            }
            // device read path is bypassed; silence "unused" for the None path vars
            let _ = (input_read_layout, input_channels);
        }
        None => {
            for frame in data.chunks(input_total_channels).take(num_frames) {
                let raw_frame = read_input_frame(*input_read_layout, input_channels, frame);
                let chain_frame = match (*input_read_layout, *processing_layout) {
                    (AudioChannelLayout::Mono, AudioChannelLayout::Stereo) => {
                        let sample = match raw_frame {
                            AudioFrame::Mono(s) => s,
                            _ => unreachable!(),
                        };
                        AudioFrame::Stereo([sample, sample])
                    }
                    _ => raw_frame,
                };
                frame_buffer.push(chain_frame);
            }
        }
    }
```

Update the call site (~line 203) to pass `di_for_seg`:

```rust
        process_single_segment(
            input_states,
            &mut scratch,
            seg_idx,
            data,
            input_total_channels,
            num_frames,
            &runtime.error_queue,
            &stream_taps,
            di_for_seg,
        );
```

After the segment loop, BEFORE snapshotting routes (~line 215), advance the cursor once:

```rust
    if let Some(d) = di_ref {
        let len = d.len().max(1);
        let next = di_start.wrapping_add(num_frames) % len;
        runtime.di_loop_pos.store(next, Ordering::Relaxed);
    }
```

- [ ] **Step 4: Run to verify it passes**

Run: `cargo test -p engine di_loop_injection`
Expected: PASS (3 tests).

- [ ] **Step 5: Run the full engine suite (invariants must stay green)**

Run: `cargo test -p engine`
Expected: PASS — especially `audio_alloc_invariant`, `volume_invariants`, `stream_isolation`, golden/signal-integrity. If `audio_alloc_invariant` fails, the injection allocated — recheck (no `to_vec`, no per-callback Vec growth).

- [ ] **Step 6: Commit**

```bash
git add crates/engine/src/runtime.rs crates/engine/src/di_loop_injection_tests.rs
git commit -m "feat(#614): inject per-chain DI loop at input read (lock-free, zero-alloc)"
git push
```
Then `gh issue comment 614` (include the alloc-invariant + isolation results).

---

## Task 4: Off-thread decode → `DiLoop` (source preload)

**Files:**
- Create the preload glue in the side-effect/application layer that turns a `DiLoopSource` into an `Arc<DiLoop>`.
- Reuse the WAV decoder in `crates/adapter-render/src/wav.rs`.
- Test: a unit test that decodes a tiny generated WAV (or fixture) and asserts a non-empty `DiLoop` at the engine SR.

- [ ] **Step 1 (EXECUTION prep): read the decoder API**

Open `crates/adapter-render/src/wav.rs` and note the read function signature (returns samples + sample rate + channel count). Confirm the engine sample rate source (passed from the active stream config — `infra-cpal` resolved config; the side-effect already knows the running SR).

- [ ] **Step 2: Write the failing test**

In the crate that will host the preload (application), add a test that writes a 1-channel WAV with a known ramp to a temp file, calls `load_di_loop(path, engine_sr)`, and asserts `di.len() > 0` and layout Mono. (If `adapter-render` is the natural home for decode, host the glue there and re-export to application.)

```rust
#[test]
fn load_di_loop_decodes_wav_to_engine_sr() {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("di.wav");
    write_test_wav_mono(&path, 24_000, &[0.0, 0.5, 1.0, 0.5]); // helper
    let di = load_di_loop(&DiLoopSource::File(path), 48_000).expect("decode");
    assert!(di.len() >= 7); // ~doubled by resample
}
```

- [ ] **Step 3: Run to verify it fails**

Run: `cargo test -p <host-crate> load_di_loop`
Expected: FAIL — `load_di_loop` / `DiLoopSource` not found.

- [ ] **Step 4: Implement `load_di_loop`**

```rust
use std::path::PathBuf;
use std::sync::Arc;
use engine::DiLoop;

/// Where a chain's DI loop comes from. `Bundled` resolves to a file under the
/// app's `assets/di-loops/`; `File` is a user-picked path. Ephemeral.
#[derive(Clone, Debug, PartialEq)]
pub enum DiLoopSource {
    Bundled(String), // bundled loop id (file stem under assets/di-loops/)
    File(PathBuf),
}

/// Default loop crossfade at the seam (frames @ engine SR). ~10 ms @ 48 kHz.
const DI_LOOP_XFADE_FRAMES: usize = 480;

/// Decode + resample + crossfade a DI source into an `Arc<DiLoop>`. Runs OFF
/// the audio thread (called from the command side-effect / a worker).
pub fn load_di_loop(source: &DiLoopSource, engine_sr: u32) -> Result<Arc<DiLoop>, String> {
    let path = resolve_di_path(source)?;
    // Reuse adapter-render's WAV reader (EXECUTION: match its real signature).
    let decoded = adapter_render::wav::read_wav(&path).map_err(|e| e.to_string())?;
    let di = DiLoop::from_samples(
        &decoded.samples,
        decoded.sample_rate,
        decoded.channels as usize,
        engine_sr,
        DI_LOOP_XFADE_FRAMES,
    );
    Ok(Arc::new(di))
}
```

`resolve_di_path` maps `Bundled(id)` to the per-platform assets dir (NEVER hardcode paths — use the existing assets-dir resolver, same one block/IR assets use; see `docs/architecture.md` "assets"). `File(p)` returns `p`.

- [ ] **Step 5: Run to verify it passes**

Run: `cargo test -p <host-crate> load_di_loop`
Expected: PASS.

- [ ] **Step 6: Commit + push + issue comment** (as before).

---

## Task 5: Commands `SetChainDiLoopSource` + `SetChainDiLoopEnabled`

**Files:**
- Modify: `crates/application/src/command.rs`
- Test: the command/dispatch test module used by `application`.

- [ ] **Step 1 (EXECUTION prep): read `command.rs`**

Open `crates/application/src/command.rs`. Find a sibling runtime-targeting command that reaches a `ChainRuntimeState` (e.g. the one behind `set_output_muted` / `set_chain_volume`) and copy its shape: variant, how the dispatcher resolves the target chain runtime, how the side-effect runs off-thread.

- [ ] **Step 2: Write the failing test**

Mirror an existing command test: dispatch `SetChainDiLoopSource { chain, source }` then `SetChainDiLoopEnabled { chain, enabled: true }` and assert the targeted chain runtime ends with `has_di_loop() == true`; then `enabled: false` ⇒ `has_di_loop() == false`. (No `AppWindow`; pure dispatch path.)

- [ ] **Step 3: Run to verify it fails.** `cargo test -p application di_loop` → FAIL.

- [ ] **Step 4: Implement the variants + dispatch**

Add to the `Command` enum:

```rust
    /// Select (and load, off-thread) the virtual DI loop for a chain. Ephemeral.
    SetChainDiLoopSource { chain: ChainId, source: DiLoopSource },
    /// Start/stop the chain's virtual DI loop (play/stop). Enabling with no
    /// source loaded is a no-op.
    SetChainDiLoopEnabled { chain: ChainId, enabled: bool },
```

Dispatch:
- `SetChainDiLoopSource`: in the side-effect, `load_di_loop(&source, engine_sr)`; on success, remember it as the chain's pending loop (held in app state, ephemeral) but do NOT publish yet unless currently enabled. On error, surface a user-visible error (no silent failure).
- `SetChainDiLoopEnabled { enabled: true }`: publish the loaded `Arc<DiLoop>` to the chain runtime via `runtime.set_di_loop(Some(di))`. If none loaded, no-op.
- `SetChainDiLoopEnabled { enabled: false }`: `runtime.set_di_loop(None)`.

Keep the loaded `Arc<DiLoop>` + chosen source in ephemeral app state keyed by `ChainId` (never serialized into the project).

- [ ] **Step 5: Run to verify it passes.** `cargo test -p application di_loop` → PASS.

- [ ] **Step 6: MCP parity check.** Confirm the new variants are reachable via the MCP adapter the same way sibling commands are (the architecture auto-derives tools from `Command`). If a manual registration list exists, add the two tools there.

- [ ] **Step 7: Commit + push + issue comment.**

---

## Task 6: GUI event → command wiring (pure handler)

**Files:**
- Modify: `crates/adapter-gui/**` (the chain-tile callbacks, next to the volume callback).
- Test: `crates/adapter-gui/tests/issue_614_di_loop_event.rs`

- [ ] **Step 1 (EXECUTION prep): read the volume callback**

Find how the chain-tile volume control dispatches (the `set_chain_volume` callback → `Event` → `Command`). Copy that exact pattern for: (a) choosing a source, (b) play/stop toggle.

- [ ] **Step 2: Write the failing test**

Test the pure event→command mapping (no `AppWindow`): given a "play DI on chain X with source S" event, the handler produces `[SetChainDiLoopSource{X,S}, SetChainDiLoopEnabled{X,true}]`; a "stop" event produces `[SetChainDiLoopEnabled{X,false}]`.

- [ ] **Step 3: Run to verify it fails.**

- [ ] **Step 4: Implement the pure handler** mapping the events to the commands above, mirroring the volume handler.

- [ ] **Step 5: Run to verify it passes.**

- [ ] **Step 6: Commit + push + issue comment.**

---

## Task 7: Chains-screen Slint control (icon next to volume → select + play/stop)

**Files:**
- Modify: the Chains chain-tile `.slint` that renders the per-chain volume control (locate via the volume property/callback name found in Task 6).
- Modify: the Rust that binds the tile's callbacks (adapter-gui).

- [ ] **Step 1: Add the icon button + select + play/stop to the tile**

- SVG icon via `@image-url` + colorize (NEVER a glyph — Orange Pi tofu). Place it adjacent to the existing volume control.
- Clicking the icon reveals a `ComboBox`/select bound to the available loops (bundled ids + a "Choose file…" entry) and a play button.
- Play button toggles to a stop button based on a per-chain `di-loop-playing` boolean property fed from the model.
- Callbacks: `di-loop-source-selected(chain, source)`, `di-loop-play(chain)`, `di-loop-stop(chain)` → Rust dispatch from Task 6.

- [ ] **Step 2: Wire the model** so `di-loop-playing` reflects `runtime.has_di_loop()` for that chain (polled with the existing per-chain UI refresh, same place stream-count/volume are read). Keep visual consistency with the other tile controls.

- [ ] **Step 3: Manual verification** (build + run): `cargo run -p adapter-gui` (or the app entry), open Chains, click the icon, pick a bundled loop, press play → audio loops through the chain; press stop → silence/real input. Confirm no click on play/stop (fade is handled by the chain's rebuild-free publish + the loop crossfade).

- [ ] **Step 4: Commit + push + issue comment.**

---

## Task 8: Bundle CC0 DI loop(s)

**Files:**
- Create: `assets/di-loops/<name>.wav` (1-2 files), `assets/di-loops/README.md` (license + attribution per file).

- [ ] **Step 1:** Source 1-2 CC0 / public-domain dry guitar DI loops. **Present each file's license/source to the user for approval BEFORE committing** (open item in the spec).
- [ ] **Step 2:** Place under `assets/di-loops/`; ensure the bundled-id resolver (Task 4 `resolve_di_path`) finds them via the platform assets dir.
- [ ] **Step 3:** Add `assets/di-loops/README.md` documenting source + license + that they are dry DIs intended for tone-shaping.
- [ ] **Step 4: Commit + push + issue comment.**

---

## Task 9: Docs

**Files:**
- Modify: `docs/screens.md` (Chains screen — document the DI-loop icon, select, play/stop, ephemeral behavior).
- Modify: `docs/audio-config.md` if the DI loop interacts with I/O docs.

- [ ] **Step 1:** Document the feature: per-chain, ephemeral (not saved), dry-DI requirement, bundled + user-file sources, play/stop next to volume.
- [ ] **Step 2:** Note the audio-thread guarantees (lock-free/zero-alloc) briefly, and that it is distinct from #324.
- [ ] **Step 3: Commit + push + issue comment.**

---

## Self-Review

**Spec coverage:**
- Injection at input read, lock-free/zero-alloc → Task 3 (+ alloc-invariant gate).
- Mono→broadcast / stereo direct (#5) → Task 3 match arms + Task 1 layout.
- Off = byte-identical passthrough (#9) → Task 3 `off_is_silent_passthrough` + full suite.
- Per-chain isolation (#4) → state lives on one `ChainRuntimeState`; full-suite `stream_isolation`.
- Ephemeral, not persisted → app-state-only in Task 5 (no project serialization).
- Off-thread decode→resample→crossfade → Tasks 1 + 4.
- Two transport-agnostic Commands + MCP parity → Task 5.
- UI icon next to volume + select + play/stop → Tasks 6-7.
- Bundled CC0 + user file → Tasks 4 + 8.
- Seamless loop (#3) → Task 1 crossfade test + Task 7 manual check.
- Docs → Task 9.

**Placeholder scan:** Tasks 1-3 are fully coded. Tasks 4-7 carry concrete test intent + the exact code where it is knowable without reading the (hook-locked) files; each begins with an explicit "read the sibling file at execution" step, because the OpenRig red-first hook blocks reading production code before a RED test exists. This is a deliberate, documented consequence of the gate — not an unbounded TODO.

**Type consistency:** `DiLoop`, `DiFrame`, `from_samples(samples, src_sr, channels, engine_sr, xfade_frames)`, `frame_at`, `set_di_loop(Option<Arc<DiLoop>>)`, `has_di_loop`, `di_loop_pos`, `DiLoopSource::{Bundled,File}`, `load_di_loop(&DiLoopSource, u32)`, Commands `SetChainDiLoopSource{chain,source}` / `SetChainDiLoopEnabled{chain,enabled}` are used consistently across tasks.
