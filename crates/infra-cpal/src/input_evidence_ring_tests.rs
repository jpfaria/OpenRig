use std::time::SystemTime;

use super::InputEvidenceRing;
use crate::input_evidence::{CycleRecord, InputStreamIdentity};

fn identity(channels: usize) -> InputStreamIdentity {
    InputStreamIdentity {
        chain_id: "chain:ring".into(),
        input_index: 0,
        device_id: Some("coreaudio:test".into()),
        sample_rate: 48_000,
        buffer_frames: 2,
        channels,
        opened_at: SystemTime::UNIX_EPOCH,
    }
}

/// Interleaved frames `first..first + frames` of a ramp where channel `c` of
/// frame `f` holds `f * 10 + c`.
fn ramp(first: u64, frames: usize, channels: usize) -> Vec<f32> {
    (0..frames)
        .flat_map(|f| (0..channels).map(move |c| ((first as usize + f) * 10 + c) as f32))
        .collect()
}

#[test]
fn snapshot_returns_every_cycle_in_order_before_the_ring_fills() {
    let ring = InputEvidenceRing::with_capacity(identity(2), 16, 8);
    for cycle in 0..3u64 {
        ring.record(&ramp(cycle * 2, 2, 2), 1_000 + cycle);
    }

    let snap = ring.snapshot();

    assert_eq!(snap.first_frame, 0);
    assert_eq!(snap.samples, ramp(0, 6, 2));
    assert_eq!(
        snap.cycles,
        vec![
            CycleRecord {
                host_ns: 1_000,
                first_frame: 0,
                frames: 2
            },
            CycleRecord {
                host_ns: 1_001,
                first_frame: 2,
                frames: 2
            },
            CycleRecord {
                host_ns: 1_002,
                first_frame: 4,
                frames: 2
            },
        ]
    );
}

#[test]
fn after_wrapping_the_snapshot_keeps_only_frames_no_cycle_can_still_overwrite() {
    // 16 frames of room, 2-frame cycles: a reader copying while the writer
    // runs could see the oldest 2 cycles (4 frames) overwritten, so the
    // snapshot leaves them out.
    let ring = InputEvidenceRing::with_capacity(identity(1), 16, 64);
    for cycle in 0..20u64 {
        ring.record(&ramp(cycle * 2, 2, 1), cycle);
    }

    let snap = ring.snapshot();

    assert_eq!(snap.first_frame, 40 - 16 + 4);
    assert_eq!(snap.samples, ramp(28, 12, 1));
}

#[test]
fn cycle_slots_wrap_and_keep_the_newest() {
    let ring = InputEvidenceRing::with_capacity(identity(1), 1_024, 4);
    for cycle in 0..10u64 {
        ring.record(&ramp(cycle, 1, 1), cycle);
    }

    let snap = ring.snapshot();

    // 4 slots, the oldest 2 are left out for the same tear margin.
    let host: Vec<u64> = snap.cycles.iter().map(|c| c.host_ns).collect();
    assert_eq!(host, vec![8, 9]);
}

#[test]
fn a_callback_with_a_partial_frame_records_only_whole_frames() {
    let ring = InputEvidenceRing::with_capacity(identity(2), 16, 8);
    ring.record(&[1.0, 2.0, 3.0], 7);

    let snap = ring.snapshot();

    assert_eq!(snap.samples, vec![1.0, 2.0]);
    assert_eq!(
        snap.cycles,
        vec![CycleRecord {
            host_ns: 7,
            first_frame: 0,
            frames: 1
        }]
    );
}

#[test]
fn an_empty_ring_gives_an_empty_snapshot() {
    let ring = InputEvidenceRing::with_capacity(identity(2), 16, 8);

    let snap = ring.snapshot();

    assert!(snap.samples.is_empty());
    assert!(snap.cycles.is_empty());
}

#[test]
fn the_default_ring_holds_three_seconds() {
    let ring = InputEvidenceRing::new(identity(2));

    assert_eq!(ring.capacity_frames(), 3 * 48_000);
}
