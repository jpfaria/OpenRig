//! #771: RT-safe, output-clocked playback of the DI worker's stream.
//!
//! One [`DiPlaybackCell`] exists per chain output stream. Arming parks a
//! [`DiPlayback`] in the CHOSEN output's cell; the DI worker keeps the
//! playback's SPSC ring topped up (paced by ring backpressure — the ring
//! level IS the clock, so it can never drift), and that output device's
//! callback pops frames and sums them into its buffer. Zero alloc/lock in
//! the callback: an `ArcSwapOption` load, SPSC pops and relaxed atomics.

use std::sync::atomic::{AtomicU32, AtomicU64, Ordering};
use std::sync::Arc;

use arc_swap::ArcSwapOption;
use engine::runtime_dsp::output_limiter;
use engine::spsc::SpscRing;

/// Ring capacity in FRAMES (interleaved L,R — 2 slots per frame). ~740 ms at
/// 44.1 kHz: rides out interactive-session preemption bursts (the owner's
/// live probe showed 200-300 ms starvation dips at normal priority) while
/// still stopping fast on disarm (the cell empties instantly).
pub(crate) const DI_RING_FRAMES: usize = 32768;

/// The DI stream parked on one output: the worker-fed ring plus the meter
/// peaks. The audio callback pops; the worker pushes.
pub(crate) struct DiPlayback {
    /// Interleaved stereo samples (L,R per frame). Single producer (the DI
    /// worker), single consumer (the output callback).
    ring: Arc<SpscRing<f32>>,
    /// Device-frame channel offsets the L/R land on.
    dest_left: usize,
    dest_right: usize,
    /// One loop period at the stream's rate, in frames (UI info).
    loop_len: usize,
    /// Loop position of this playback's FIRST frame. A gapless re-arm (#785)
    /// starts its render mid-loop, where the outgoing playback will be at
    /// hand-off time.
    start_pos: usize,
    /// Frames the callback has popped (Relaxed, incremented in the mix). With
    /// `start_pos` this gives the loop position the listener is hearing —
    /// what the incoming render of a gapless re-arm has to line up with.
    consumed: AtomicU64,
    /// f32 bits of the last window's peaks (Relaxed). `in` is published by
    /// the worker (raw loop), `out` by the callback (mixed frames).
    in_peak_bits: AtomicU32,
    out_peak_bits: AtomicU32,
}

/// Per-output-stream slot the callback loads wait-free. `None` = no DI parked.
pub(crate) type DiPlaybackCell = Arc<ArcSwapOption<DiPlayback>>;

/// Playbacks swapped out by a disarm or a gapless hand-off (#785), freed on a
/// LATER cycle. Shared with the DI worker, which retires the outgoing playback
/// itself when it takes the cell over.
pub(crate) type DiRetired = Arc<std::sync::Mutex<Vec<Arc<DiPlayback>>>>;

impl DiPlayback {
    pub(crate) fn new(dest_left: usize, dest_right: usize, loop_len: usize) -> Self {
        Self::starting_at(dest_left, dest_right, loop_len, 0)
    }

    /// A playback whose first frame is loop position `start_pos` (#785).
    pub(crate) fn starting_at(
        dest_left: usize,
        dest_right: usize,
        loop_len: usize,
        start_pos: usize,
    ) -> Self {
        Self {
            ring: Arc::new(SpscRing::new(DI_RING_FRAMES * 2, 0.0)),
            dest_left,
            dest_right,
            loop_len,
            start_pos,
            consumed: AtomicU64::new(0),
            in_peak_bits: AtomicU32::new(0),
            out_peak_bits: AtomicU32::new(0),
        }
    }

    /// The loop position the listener is currently hearing.
    pub(crate) fn play_pos(&self) -> usize {
        let loop_len = self.loop_len.max(1) as u64;
        let consumed = self.consumed.load(Ordering::Relaxed);
        ((self.start_pos as u64 + consumed) % loop_len) as usize
    }

    /// The worker's handle to the ring (producer side).
    pub(crate) fn ring(&self) -> Arc<SpscRing<f32>> {
        Arc::clone(&self.ring)
    }

    /// Worker-side: publish the raw loop's peak for the DI IN meter.
    pub(crate) fn set_in_peak(&self, peak: f32) {
        self.in_peak_bits.store(peak.to_bits(), Ordering::Relaxed);
    }

    /// Linear `(in, out)` peaks of the last window (DI meter row).
    pub(crate) fn peaks(&self) -> (f32, f32) {
        (
            f32::from_bits(self.in_peak_bits.load(Ordering::Relaxed)),
            f32::from_bits(self.out_peak_bits.load(Ordering::Relaxed)),
        )
    }

    /// One loop period, in frames.
    pub(crate) fn loop_len(&self) -> usize {
        self.loop_len
    }
}

/// Sum the parked DI stream into `out` (interleaved, `output_total_channels`
/// wide). Pops whole frames only (no L/R skew); an under-filled ring leaves
/// the remaining frames untouched (the worker is catching up). No-op when
/// the cell is empty. Runs on the output audio callback — zero alloc/lock.
pub(crate) fn mix_di_playback(
    cell: &DiPlaybackCell,
    out: &mut [f32],
    output_total_channels: usize,
) {
    let guard = cell.load();
    let Some(playback) = guard.as_ref() else {
        return;
    };
    if output_total_channels == 0 {
        return;
    }
    let mut out_peak = 0.0f32;
    let mut popped = 0u64;
    for frame in out.chunks_mut(output_total_channels) {
        // A whole frame (2 samples) or stop — the producer pushes whole
        // frames, so fewer than 2 readable samples means "mid-push"; leave
        // it for the next callback rather than skewing channels.
        if playback.ring.len() < 2 {
            break;
        }
        let (Some(l), Some(r)) = (playback.ring.pop(), playback.ring.pop()) else {
            break;
        };
        out_peak = out_peak.max(l.abs()).max(r.abs());
        popped += 1;
        if let Some(s) = frame.get_mut(playback.dest_left) {
            *s = output_limiter(*s + l);
        }
        // A mono dest (dest_right == dest_left) already carries the frame —
        // a second add would be +6 dB over the chain's own rendering.
        if playback.dest_right != playback.dest_left {
            if let Some(s) = frame.get_mut(playback.dest_right) {
                *s = output_limiter(*s + r);
            }
        }
    }
    playback.consumed.fetch_add(popped, Ordering::Relaxed);
    playback
        .out_peak_bits
        .store(out_peak.to_bits(), Ordering::Relaxed);
}

#[cfg(test)]
mod tests {
    use super::*;

    fn cell_with(playback: DiPlayback) -> (DiPlaybackCell, Arc<SpscRing<f32>>) {
        let ring = playback.ring();
        (
            Arc::new(ArcSwapOption::from(Some(Arc::new(playback)))),
            ring,
        )
    }

    /// Push whole frames; values exactly representable in f32.
    fn push_frames(ring: &SpscRing<f32>, frames: &[[f32; 2]]) {
        for f in frames {
            assert!(ring.push(f[0]));
            assert!(ring.push(f[1]));
        }
    }

    #[test]
    fn mix_pops_frames_onto_dest_channels() {
        let (cell, ring) = cell_with(DiPlayback::new(0, 1, 8));
        push_frames(&ring, &[[0.03125, -0.03125], [0.0625, -0.0625]]);
        let mut out = vec![0.0f32; 2 * 2];
        mix_di_playback(&cell, &mut out, 2);
        assert_eq!(out, vec![0.03125, -0.03125, 0.0625, -0.0625]);
    }

    #[test]
    fn underfilled_ring_leaves_remaining_frames_untouched() {
        let (cell, ring) = cell_with(DiPlayback::new(0, 1, 8));
        push_frames(&ring, &[[0.25, 0.25]]);
        let mut out = vec![0.0f32; 3 * 2];
        mix_di_playback(&cell, &mut out, 2);
        assert_eq!(
            out,
            vec![0.25, 0.25, 0.0, 0.0, 0.0, 0.0],
            "only the available frame plays; the rest stays silent"
        );
    }

    #[test]
    fn mix_targets_only_its_dest_channels() {
        let (cell, ring) = cell_with(DiPlayback::new(2, 3, 8));
        push_frames(&ring, &[[0.03125, -0.03125]]);
        let mut out = vec![0.0f32; 4];
        mix_di_playback(&cell, &mut out, 4);
        assert_eq!(out, vec![0.0, 0.0, 0.03125, -0.03125]);
    }

    #[test]
    fn mix_sums_over_existing_signal_with_the_output_limiter() {
        let (cell, ring) = cell_with(DiPlayback::new(0, 1, 8));
        push_frames(&ring, &[[0.9, 0.9]; 4]);
        let mut out = vec![0.9f32; 4 * 2];
        mix_di_playback(&cell, &mut out, 2);
        for (i, s) in out.iter().enumerate() {
            assert!(*s <= 1.0, "sample {i} must stay limited, got {s}");
            assert!(*s > 0.9, "sample {i} must be the SUM (limited), got {s}");
        }
    }

    /// #771 review: a MONO output endpoint has dest_left == dest_right and
    /// the frame is [m, m] — the sample must be written ONCE, not summed
    /// twice (+6 dB vs the chain's own rendering).
    #[test]
    fn mono_dest_is_written_once_not_summed_twice() {
        let (cell, ring) = cell_with(DiPlayback::new(0, 0, 8));
        push_frames(&ring, &[[0.25, 0.25]]);
        let mut out = vec![0.0f32; 1];
        mix_di_playback(&cell, &mut out, 1);
        assert_eq!(out[0], 0.25, "mono dest carries the frame once, not 2x");
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
        let (cell, ring) = cell_with(DiPlayback::new(0, 1, 8));
        push_frames(&ring, &[[0.25, -0.125], [0.0625, 0.0]]);
        let mut out = vec![0.0f32; 2 * 2];
        mix_di_playback(&cell, &mut out, 2);
        let playback = cell.load();
        let playback = playback.as_ref().expect("parked");
        playback.set_in_peak(0.5);
        let (in_peak, out_peak) = playback.peaks();
        assert!((out_peak - 0.25).abs() < 1e-6, "out peak, got {out_peak}");
        assert_eq!(in_peak, 0.5, "in peak comes from the worker");
    }
}
