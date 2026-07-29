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
        content_rev: 0,
    }
}

#[test]
fn a_looper_with_no_runtime_row_renders_as_empty() {
    let chain = chain_with(vec![LooperConfig::new(1)]);
    let rows = looper_items(&chain, &[], 48_000, &[], true, &[]);

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
fn rec_is_offered_only_when_the_runtime_is_live() {
    // REC captures the live input through the chain's runtime, so it is armable
    // only when that runtime is actually LIVE — not merely when the chain is
    // enabled. `enabled` is not enough: right after reopening a project the
    // runtime is still cold-starting, and a REC press then records nothing.
    let mut chain = chain_with(vec![LooperConfig::new(1)]);
    chain.enabled = true;

    assert!(
        looper_items(&chain, &[], 48_000, &[], true, &[])[0].can_record,
        "a live runtime offers REC"
    );
    assert!(
        !looper_items(&chain, &[], 48_000, &[], false, &[])[0].can_record,
        "no live runtime ⇒ REC disabled even for an enabled chain"
    );
    // The config-only (project-open) path never has a live runtime yet.
    assert!(
        !looper_items_from_config(&chain, &[], &[])[0].can_record,
        "no live runtime yet ⇒ REC disabled"
    );
}

#[test]
fn single_take_rec_is_disabled_once_the_loop_has_material() {
    // #323: single-take looper — after a recording is closed, REC must be
    // disabled (there is no overdub; the user clears to re-record). It is armable
    // only on an EMPTY loop (with a live runtime) or to CLOSE one in progress.
    let chain = chain_with(vec![LooperConfig::new(1)]);
    let rec = |state| {
        looper_items(&chain, &[status(1, state, 0, 100, 1)], 48_000, &[], true, &[])[0].can_record
    };
    assert!(
        rec(LooperState::Recording),
        "REC stays enabled to CLOSE an in-progress recording"
    );
    assert!(
        !rec(LooperState::Playing),
        "a recorded, playing loop cannot re-record (no overdub)"
    );
    assert!(
        !rec(LooperState::Stopped),
        "a recorded, stopped loop cannot re-record — clear first"
    );
}

#[test]
fn live_state_progress_and_time_come_from_the_runtime_at_the_live_rate() {
    let chain = chain_with(vec![LooperConfig::new(1)]);
    let rows = looper_items(
        &chain,
        &[status(1, LooperState::Playing, 48_000, 384_000, 3)],
        48_000,
        &[],
        true,
        &[],
    );

    assert_eq!(rows[0].state_code, 2);
    assert_eq!(rows[0].layers, 3);
    assert_eq!(rows[0].progress, 0.125);
    assert_eq!(rows[0].time_label, "0:01 / 0:08");
}

#[test]
fn time_label_follows_a_44100_stream_not_a_hardcoded_48000() {
    let chain = chain_with(vec![LooperConfig::new(1)]);
    let rows = looper_items(
        &chain,
        &[status(1, LooperState::Playing, 0, 44_100 * 5, 1)],
        44_100,
        &[],
        true,
        &[],
    );
    assert_eq!(rows[0].time_label, "0:00 / 0:05");
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
        input: None,
        output: None,
        preset: None,
    }]);
    let rows = looper_items(&chain, &[], 48_000, &[], true, &[]);

    assert_eq!(rows[0].mix, 50);
    assert_eq!(rows[0].decay, 25);
    assert_eq!(rows[0].speed_index, 2);
    assert!(rows[0].reverse);
}

#[test]
fn a_chain_is_active_while_any_looper_records_or_plays() {
    let chain = chain_with(vec![LooperConfig::new(1), LooperConfig::new(2)]);
    assert!(!any_looper_active(&looper_items(&chain, &[], 48_000, &[], true, &[])));

    let rows = looper_items(
        &chain,
        &[status(2, LooperState::Recording, 0, 0, 1)],
        48_000,
        &[],
        true,
        &[],
    );
    assert!(any_looper_active(&rows));

    let stopped = looper_items(
        &chain,
        &[status(2, LooperState::Stopped, 0, 48_000, 1)],
        48_000,
        &[],
        true,
        &[],
    );
    assert!(
        !any_looper_active(&stopped),
        "a stopped looper is not making sound"
    );
}
