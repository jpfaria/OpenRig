//! #323 — the pure mapping from (persisted config + live status) to the rows
//! the panel renders.

use super::*;
use engine::{LooperState, LooperStatus};
use project::chain::{Chain, LooperConfig, LooperSpeed};

fn chain_with(loopers: Vec<LooperConfig>) -> Chain {
    Chain {
        id: domain::ids::ChainId("c1".into()),
        description: None,
        instrument: "electric_guitar".into(),
        enabled: true,
        volume: 100.0,
        io_binding_ids: vec![],
        blocks: vec![],
        di_output: None,
        loopers,
    }
}

fn status(
    uid: u64,
    state: LooperState,
    position: usize,
    len: usize,
    layers: usize,
) -> LooperStatus {
    LooperStatus {
        uid,
        state,
        position_frames: position,
        len_frames: len,
        layers,
    }
}

#[test]
fn a_looper_with_no_runtime_row_renders_as_empty() {
    let chain = chain_with(vec![LooperConfig::new(1)]);
    let rows = looper_items(&chain, &[], 48_000);

    assert_eq!(rows.len(), 1);
    assert_eq!(rows[0].uid, 1);
    assert_eq!(rows[0].state_code, 0);
    assert_eq!(rows[0].layers, 0);
    assert_eq!(rows[0].progress, 0.0);
    assert_eq!(rows[0].time_label, "0:00 / 0:00");
    assert!(!rows[0].can_undo);
    assert!(!rows[0].can_redo);
}

#[test]
fn live_state_progress_and_time_come_from_the_runtime_at_the_live_rate() {
    let chain = chain_with(vec![LooperConfig::new(1)]);
    let rows = looper_items(
        &chain,
        &[status(1, LooperState::Playing, 48_000, 384_000, 3)],
        48_000,
    );

    assert_eq!(rows[0].state_code, 2);
    assert_eq!(rows[0].layers, 3);
    assert_eq!(rows[0].progress, 0.125);
    assert_eq!(rows[0].time_label, "0:01 / 0:08");
    assert!(rows[0].can_undo, "there are layers to undo");
}

#[test]
fn time_label_follows_a_44100_stream_not_a_hardcoded_48000() {
    let chain = chain_with(vec![LooperConfig::new(1)]);
    let rows = looper_items(
        &chain,
        &[status(1, LooperState::Playing, 0, 44_100 * 5, 1)],
        44_100,
    );
    assert_eq!(rows[0].time_label, "0:00 / 0:05");
}

#[test]
fn redo_is_offered_only_while_an_undone_layer_is_still_there() {
    let chain = chain_with(vec![LooperConfig::new(1)]);
    // 2 layers recorded, 1 audible ⇒ the undone one can come back.
    let mut rows = looper_items(
        &chain,
        &[status(1, LooperState::Playing, 0, 48_000, 1)],
        48_000,
    );
    assert!(!rows[0].can_redo, "nothing is known to be undone yet");

    rows = looper_items_with_recorded(
        &chain,
        &[status(1, LooperState::Playing, 0, 48_000, 1)],
        48_000,
        &[(1u64, 2usize)],
    );
    assert!(rows[0].can_redo);
}

#[test]
fn persisted_parameters_reach_the_row_in_panel_units() {
    let chain = chain_with(vec![LooperConfig {
        uid: 4,
        mix: 0.5,
        decay: 0.25,
        speed: LooperSpeed::Double,
        reverse: true,
        audio_file: None,
    }]);
    let rows = looper_items(&chain, &[], 48_000);

    assert_eq!(rows[0].mix, 50);
    assert_eq!(rows[0].decay, 25);
    assert_eq!(rows[0].speed_index, 2);
    assert!(rows[0].reverse);
}

#[test]
fn a_chain_is_active_while_any_looper_records_or_plays() {
    let chain = chain_with(vec![LooperConfig::new(1), LooperConfig::new(2)]);
    assert!(!any_looper_active(&looper_items(&chain, &[], 48_000)));

    let rows = looper_items(
        &chain,
        &[status(2, LooperState::Recording, 0, 0, 1)],
        48_000,
    );
    assert!(any_looper_active(&rows));

    let stopped = looper_items(
        &chain,
        &[status(2, LooperState::Stopped, 0, 48_000, 1)],
        48_000,
    );
    assert!(
        !any_looper_active(&stopped),
        "a stopped looper is not making sound"
    );
}
