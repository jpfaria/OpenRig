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
    let (cell, ring) = cell_with(DiPlayback::starting_at(0, 1, 8, 0));
    push_frames(&ring, &[[0.03125, -0.03125], [0.0625, -0.0625]]);
    let mut out = vec![0.0f32; 2 * 2];
    mix_di_playback(&cell, &mut out, 2);
    assert_eq!(out, vec![0.03125, -0.03125, 0.0625, -0.0625]);
}

#[test]
fn underfilled_ring_leaves_remaining_frames_untouched() {
    let (cell, ring) = cell_with(DiPlayback::starting_at(0, 1, 8, 0));
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
    let (cell, ring) = cell_with(DiPlayback::starting_at(2, 3, 8, 0));
    push_frames(&ring, &[[0.03125, -0.03125]]);
    let mut out = vec![0.0f32; 4];
    mix_di_playback(&cell, &mut out, 4);
    assert_eq!(out, vec![0.0, 0.0, 0.03125, -0.03125]);
}

#[test]
fn mix_sums_over_existing_signal_with_the_output_limiter() {
    let (cell, ring) = cell_with(DiPlayback::starting_at(0, 1, 8, 0));
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
    let (cell, ring) = cell_with(DiPlayback::starting_at(0, 0, 8, 0));
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
    let (cell, ring) = cell_with(DiPlayback::starting_at(0, 1, 8, 0));
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
