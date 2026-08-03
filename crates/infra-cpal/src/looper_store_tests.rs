//! #323 — the controller-owned looper store drives the whole loop lifecycle
//! with NO chain runtime in sight — the property the old bank-in-runtime design
//! could not hold (stop/clear only landed inside a running audio callback).

use super::*;
use application::looper_edit::SEAM_FRAMES;
use engine::LooperState;

fn cid() -> ChainId {
    ChainId("c".into())
}

#[test]
fn a_second_loop_syncs_to_the_master_length_and_phase() {
    // #323 loop-sync: the first closed loop is the MASTER. A second loop
    // quantizes its length to the nearest MULTIPLE of the master, and closing
    // it restarts every playing loop at 0 so they play locked to the same bar.
    let mut store = LooperStore::default();
    store.set_sample_rate(48_000);

    // Master: 100 frames.
    store.create(&cid(), 1);
    store.tap_record(&cid(), 1);
    store.record_frames(&cid(), 1, &vec![0.1f32; 100 * 2]);
    store.tap_record(&cid(), 1); // close → master, len 100
    let master = store.status(&cid(), 1).unwrap().len_frames;
    assert_eq!(master, 100);

    // Second loop: ~1.9x the master (190 frames) → quantizes to 2x = 200.
    store.create(&cid(), 2);
    store.tap_record(&cid(), 2);
    store.record_frames(&cid(), 2, &vec![0.2f32; 190 * 2]);
    store.tap_record(&cid(), 2); // close → quantize + phase-align

    assert_eq!(
        store.status(&cid(), 2).unwrap().len_frames,
        master * 2,
        "the second loop is a whole multiple of the master (locked to the bar)"
    );
    assert_eq!(
        store.status(&cid(), 1).unwrap().position_frames,
        0,
        "closing a synced loop restarts the master at 0 (phase-aligned)"
    );
    assert_eq!(
        store.status(&cid(), 2).unwrap().position_frames,
        0,
        "the new loop also starts at 0"
    );
}

#[test]
fn record_close_play_stop_clear_are_deterministic_without_any_runtime() {
    let mut store = LooperStore::default();
    store.create(&cid(), 1);
    assert_eq!(store.status(&cid(), 1).unwrap().state, LooperState::Empty);

    // Record: start, feed 3 stereo frames of dry audio, close.
    store.tap_record(&cid(), 1);
    assert_eq!(store.status(&cid(), 1).unwrap().state, LooperState::Recording);
    store.record_frames(&cid(), 1, &[0.2, 0.2, 0.3, 0.3, 0.4, 0.4]);
    store.tap_record(&cid(), 1); // close → Playing
    let s = store.status(&cid(), 1).unwrap();
    assert_eq!(s.state, LooperState::Playing);
    assert_eq!(s.len_frames, 3);
    assert!(store.export(&cid(), 1).is_some(), "a closed loop exports audio");

    // Stop / clear act with NO runtime — the whole point of the redesign.
    store.stop(&cid(), 1);
    assert_eq!(store.status(&cid(), 1).unwrap().state, LooperState::Stopped);
    store.clear(&cid(), 1);
    let s = store.status(&cid(), 1).unwrap();
    assert_eq!(s.state, LooperState::Empty);
    assert_eq!(s.len_frames, 0);

    store.remove(&cid(), 1);
    assert!(store.status(&cid(), 1).is_none());
}

#[test]
fn drain_recording_captures_dry_samples_from_the_tap_ring() {
    let mut store = LooperStore::default();
    store.create(&cid(), 1);
    store.tap_record(&cid(), 1); // start recording
    assert!(!store.is_recording_armed(&cid(), 1), "no rings yet");

    let ring = std::sync::Arc::new(engine::spsc::SpscRing::<f32>::new(64, 0.0));
    for s in [0.1f32, 0.2, 0.3] {
        ring.push(s);
    }
    store.set_recording_rings(&cid(), 1, vec![ring]);
    assert!(store.is_recording_armed(&cid(), 1));

    store.drain_recording(&cid(), 1); // pull the 3 samples in
    store.tap_record(&cid(), 1); // close → Playing

    let st = store.status(&cid(), 1).unwrap();
    assert_eq!(st.state, LooperState::Playing);
    assert_eq!(st.len_frames, 3, "3 mono samples became 3 stereo frames");
    assert!(
        !store.is_recording_armed(&cid(), 1),
        "closing the recording drops the rings"
    );
}

fn block(id: &str) -> AudioBlock {
    use project::block::{AudioBlockKind, CoreBlock};
    AudioBlock {
        id: domain::ids::BlockId(id.into()),
        enabled: true,
        kind: AudioBlockKind::Core(CoreBlock {
            effect_type: "gain".into(),
            model: "clean".into(),
            params: project::param::ParameterSet::default(),
        }),
    }
}

#[test]
fn playback_blocks_store_and_bump_rev_only_on_real_change() {
    // #323 phase 2: the loop's linked-preset blocks live in the store so the
    // controller stays preset-agnostic. The re-arm generation must bump when
    // the blocks change (preset edited/reassigned) and stay put on an
    // idempotent tick, so a steady loop never respawns its render.
    let mut store = LooperStore::default();
    store.create(&cid(), 1);
    assert!(store.playback_blocks(&cid(), 1).is_none());
    assert_eq!(store.playback_rev(&cid(), 1), 0);

    store.set_playback_blocks(&cid(), 1, Some(vec![block("a")]));
    let rev1 = store.playback_rev(&cid(), 1);
    assert_eq!(rev1, 1, "installing blocks bumps the generation");
    assert_eq!(store.playback_blocks(&cid(), 1).unwrap().len(), 1);

    // Same blocks again — idempotent, no bump.
    store.set_playback_blocks(&cid(), 1, Some(vec![block("a")]));
    assert_eq!(
        store.playback_rev(&cid(), 1),
        rev1,
        "an unchanged tick must not respawn the render"
    );

    // Different blocks — a real change bumps.
    store.set_playback_blocks(&cid(), 1, Some(vec![block("b")]));
    assert_eq!(store.playback_rev(&cid(), 1), rev1 + 1);

    // Clearing back to the chain's own blocks also counts as a change.
    store.set_playback_blocks(&cid(), 1, None);
    assert_eq!(store.playback_rev(&cid(), 1), rev1 + 2);
    assert!(store.playback_blocks(&cid(), 1).is_none());
}

#[test]
fn each_loop_is_isolated_and_carries_its_routing() {
    let mut store = LooperStore::default();
    store.create(&cid(), 7);
    store.set_output(
        &cid(),
        7,
        Some(EndpointRef {
            binding_id: "io".into(),
            endpoint: "out1".into(),
        }),
    );
    assert_eq!(store.output(&cid(), 7).unwrap().endpoint, "out1");
    // A different uid is untouched.
    store.create(&cid(), 8);
    assert!(store.output(&cid(), 8).is_none());
    assert_eq!(store.statuses(&cid()).len(), 2);
}

/// #826 — a stopped loop of `frames` frames, ready to be edited.
fn store_with_recorded_loop(frames: usize) -> (LooperStore, ChainId, u64) {
    let mut store = LooperStore::default();
    store.set_sample_rate(48_000);
    store.create(&cid(), 1);
    store.tap_record(&cid(), 1);
    // A ramp, so an edit is visible sample by sample.
    let pcm: Vec<f32> = (0..frames)
        .flat_map(|i| {
            let v = i as f32 / frames as f32;
            [v, -v]
        })
        .collect();
    store.record_frames(&cid(), 1, &pcm);
    store.tap_record(&cid(), 1); // close → Playing
    (store, cid(), 1)
}

#[test]
fn an_edit_is_refused_while_the_loop_is_not_stopped() {
    // #826: editing is a stopped-only operation — a live loop must never be
    // reshaped under the player's feet, and never silently stopped either.
    let (mut store, chain, uid) = store_with_recorded_loop(512);
    store.play(&chain, uid);

    assert_eq!(
        store.apply_edit(&chain, uid, LoopEdit::Trim { start: 64, end: 448 }),
        Err(LooperEditRefused::NotStopped)
    );
    assert_eq!(
        store.status(&chain, uid).unwrap().len_frames,
        512,
        "the refused edit changed nothing"
    );
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

    store
        .apply_edit(&chain, uid, LoopEdit::Cut { start: 100, end: 200 })
        .unwrap();
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
    let (mut store, chain, uid) = store_with_recorded_loop(8192);
    store.stop(&chain, uid);
    for _ in 0..LOOPER_EDIT_HISTORY_MAX + 3 {
        store
            .apply_edit(&chain, uid, LoopEdit::Cut { start: 100, end: 200 })
            .unwrap();
    }
    assert_eq!(
        store.edit_history_depth(&chain, uid).0,
        LOOPER_EDIT_HISTORY_MAX
    );
}

#[test]
fn clearing_the_loop_clears_the_edit_history() {
    // The old buffers belong to audio that no longer exists; undoing into them
    // would resurrect a take the player replaced.
    let (mut store, chain, uid) = store_with_recorded_loop(512);
    store.stop(&chain, uid);
    store
        .apply_edit(&chain, uid, LoopEdit::Trim { start: 64, end: 448 })
        .unwrap();
    assert_eq!(store.edit_history_depth(&chain, uid).0, 1);

    store.clear(&chain, uid);
    assert_eq!(store.edit_history_depth(&chain, uid), (0, 0));
}
