//! #771: RT-safe, output-clocked playback of a pre-rendered DI loop.
//!
//! One [`DiPlaybackCell`] exists per chain output stream. Arming stores a
//! [`DiPlayback`] in the CHOSEN output's cell; that output device's callback
//! calls [`mix_di_playback`] right after draining the chain runtimes, summing
//! the rendered frames at a cursor the callback itself advances — the output
//! device clock IS the DI clock, so it can never drift (the #717 revert
//! `f1131725e`). Zero alloc/lock in the callback: an `ArcSwapOption` load,
//! slice reads and relaxed atomics.

use std::sync::atomic::{AtomicU32, AtomicUsize, Ordering};
use std::sync::Arc;

use arc_swap::ArcSwapOption;
use engine::di_render::DiRenderedLoop;
use engine::runtime_dsp::output_limiter;
use engine::DiLoop;

/// A pre-rendered DI loop parked on one output stream, plus the cursor and
/// meter peaks the callback maintains.
pub(crate) struct DiPlayback {
    rendered: Arc<DiRenderedLoop>,
    /// Raw (un-processed) loop at the same rate/length — the DI IN meter.
    raw: Arc<DiLoop>,
    /// Device-frame channel offsets the rendered L/R land on.
    dest_left: usize,
    dest_right: usize,
    cursor: AtomicUsize,
    /// f32 bits of the last mixed window's peaks (Relaxed; meter poll only).
    in_peak_bits: AtomicU32,
    out_peak_bits: AtomicU32,
}

/// Per-output-stream slot the callback loads wait-free. `None` = no DI parked.
pub(crate) type DiPlaybackCell = Arc<ArcSwapOption<DiPlayback>>;

impl DiPlayback {
    pub(crate) fn new(
        rendered: Arc<DiRenderedLoop>,
        raw: Arc<DiLoop>,
        dest_left: usize,
        dest_right: usize,
    ) -> Self {
        Self {
            rendered,
            raw,
            dest_left,
            dest_right,
            cursor: AtomicUsize::new(0),
            in_peak_bits: AtomicU32::new(0),
            out_peak_bits: AtomicU32::new(0),
        }
    }

    /// Linear `(in, out)` peaks of the last mixed window (DI meter row).
    pub(crate) fn peaks(&self) -> (f32, f32) {
        (
            f32::from_bits(self.in_peak_bits.load(Ordering::Relaxed)),
            f32::from_bits(self.out_peak_bits.load(Ordering::Relaxed)),
        )
    }

    /// One steady-state loop period, in frames (the render length).
    pub(crate) fn loop_len(&self) -> usize {
        self.rendered.frames.len()
    }
}

/// Sum the parked DI loop into `out` (interleaved, `output_total_channels`
/// wide) and advance its cursor by the frames written. No-op when the cell is
/// empty. Runs on the output audio callback — zero alloc/lock.
pub(crate) fn mix_di_playback(
    cell: &DiPlaybackCell,
    out: &mut [f32],
    output_total_channels: usize,
) {
    let guard = cell.load();
    let Some(playback) = guard.as_ref() else {
        return;
    };
    let len = playback.rendered.frames.len();
    if len == 0 || output_total_channels == 0 {
        return;
    }
    let mut cursor = playback.cursor.load(Ordering::Relaxed);
    let mut in_peak = 0.0f32;
    let mut out_peak = 0.0f32;
    for frame in out.chunks_mut(output_total_channels) {
        let [l, r] = playback.rendered.frames[cursor % len];
        out_peak = out_peak.max(l.abs()).max(r.abs());
        let raw_peak = match playback.raw.frame_at(cursor) {
            engine::DiFrame::Mono(s) => s.abs(),
            engine::DiFrame::Stereo([a, b]) => a.abs().max(b.abs()),
        };
        in_peak = in_peak.max(raw_peak);
        if let Some(s) = frame.get_mut(playback.dest_left) {
            *s = output_limiter(*s + l);
        }
        if let Some(s) = frame.get_mut(playback.dest_right) {
            *s = output_limiter(*s + r);
        }
        cursor = (cursor + 1) % len;
    }
    playback.cursor.store(cursor, Ordering::Relaxed);
    playback
        .in_peak_bits
        .store(in_peak.to_bits(), Ordering::Relaxed);
    playback
        .out_peak_bits
        .store(out_peak.to_bits(), Ordering::Relaxed);
}

#[cfg(test)]
mod tests {
    use super::*;
    use engine::DiPcm;

    /// A rendered loop whose frame i is `((i+1)/32, -(i+1)/32)` — per-index
    /// recognizable AND exactly representable in f32 (binary fractions), so
    /// identity assertions can use `==`.
    fn rendered(len: usize) -> Arc<DiRenderedLoop> {
        Arc::new(DiRenderedLoop {
            frames: (0..len)
                .map(|i| [(i + 1) as f32 / 32.0, -((i + 1) as f32) / 32.0])
                .collect(),
            sample_rate: 48_000,
        })
    }

    fn raw(len: usize) -> Arc<DiLoop> {
        Arc::new(DiPcm::new(vec![0.5; len], 48_000, 1).to_loop_at(48_000))
    }

    fn cell_with(playback: DiPlayback) -> DiPlaybackCell {
        Arc::new(ArcSwapOption::from(Some(Arc::new(playback))))
    }

    #[test]
    fn mix_writes_loop_frames_on_dest_channels_and_wraps() {
        let cell = cell_with(DiPlayback::new(rendered(8), raw(8), 0, 1));
        let mut out = vec![0.0f32; 12 * 2];
        mix_di_playback(&cell, &mut out, 2);

        // Frame 0 carries rendered frame 0 on (0, 1)...
        assert_eq!(out[0], 0.03125, "frame 0 L must carry rendered[0]");
        assert_eq!(out[1], -0.03125, "frame 0 R must carry rendered[0]");
        // ...frame 7 carries rendered frame 7, and frame 8 WRAPS to frame 0.
        assert_eq!(out[7 * 2], 0.25, "frame 7 L must carry rendered[7]");
        assert_eq!(out[8 * 2], 0.03125, "frame 8 must wrap to rendered[0]");
        assert_eq!(out[11 * 2], 0.125, "frame 11 must wrap to rendered[3]");

        // Cursor persisted: the NEXT buffer continues from frame 12 % 8 = 4.
        let mut next = vec![0.0f32; 2];
        mix_di_playback(&cell, &mut next, 2);
        assert_eq!(next[0], 0.15625, "next buffer must continue at rendered[4]");
    }

    #[test]
    fn mix_targets_only_its_dest_channels() {
        let cell = cell_with(DiPlayback::new(rendered(4), raw(4), 2, 3));
        let mut out = vec![0.0f32; 4 * 4];
        mix_di_playback(&cell, &mut out, 4);

        assert_eq!(out[2], 0.03125, "dest L is channel 2");
        assert_eq!(out[3], -0.03125, "dest R is channel 3");
        assert_eq!(out[0], 0.0, "channel 0 stays untouched");
        assert_eq!(out[1], 0.0, "channel 1 stays untouched");
    }

    #[test]
    fn mix_sums_over_existing_signal_with_the_output_limiter() {
        let loud = Arc::new(DiRenderedLoop {
            frames: vec![[0.9, 0.9]; 4],
            sample_rate: 48_000,
        });
        let cell = cell_with(DiPlayback::new(loud, raw(4), 0, 1));
        let mut out = vec![0.9f32; 4 * 2];
        mix_di_playback(&cell, &mut out, 2);

        for (i, s) in out.iter().enumerate() {
            assert!(
                *s <= 1.0,
                "sample {i} must stay limited to full scale, got {s}"
            );
            assert!(
                *s > 0.9,
                "sample {i} must still be the SUM (limited), not a replace, got {s}"
            );
        }
    }

    #[test]
    fn empty_cell_leaves_buffer_untouched() {
        let cell: DiPlaybackCell = Arc::new(ArcSwapOption::from(None));
        let mut out = vec![0.25f32; 8];
        mix_di_playback(&cell, &mut out, 2);
        assert!(out.iter().all(|s| *s == 0.25), "no playback → no writes");
    }

    #[test]
    fn peaks_reflect_the_last_mixed_window() {
        let cell = cell_with(DiPlayback::new(rendered(8), raw(8), 0, 1));
        let mut out = vec![0.0f32; 8 * 2];
        mix_di_playback(&cell, &mut out, 2);

        let playback = cell.load();
        let (in_peak, out_peak) = playback.as_ref().expect("parked").peaks();
        assert!(
            (out_peak - 0.25).abs() < 1e-6,
            "out peak must be the window's max rendered magnitude, got {out_peak}"
        );
        assert!(
            in_peak > 0.0,
            "in peak must read the raw loop's magnitude, got {in_peak}"
        );
    }
}
