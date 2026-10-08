//! Responsibility: proves a block's stream data keeps its model while only the readings move
//!
//! The stream timers run every 50-80 ms while a utility block plays. A fresh
//! model per tick dirties every binding on the stream (and, in the compact
//! view, the whole block row) for nothing; the readings move inside the model
//! the panel already shows (#1099).

use slint::{Model, ModelRc, VecModel};

use super::sync_block_stream;
use crate::BlockStreamData;

fn entry(key: &str, value: f32) -> block_core::StreamEntry {
    block_core::StreamEntry {
        key: key.into(),
        value,
        text: format!("{value}"),
        peak: 0.0,
    }
}

fn shown(kind: &str, entries: &[block_core::StreamEntry]) -> BlockStreamData {
    sync_block_stream(&BlockStreamData::default(), kind.into(), entries)
        .expect("a first reading is something to show")
}

fn same_model(a: &ModelRc<crate::BlockStreamEntry>, b: &ModelRc<crate::BlockStreamEntry>) -> bool {
    match (
        a.as_any()
            .downcast_ref::<VecModel<crate::BlockStreamEntry>>(),
        b.as_any()
            .downcast_ref::<VecModel<crate::BlockStreamEntry>>(),
    ) {
        (Some(x), Some(y)) => std::ptr::eq(x, y),
        _ => false,
    }
}

#[test]
fn new_readings_with_the_same_rows_update_the_shown_model_in_place() {
    let current = shown("stream", &[entry("note", 1.0), entry("cents", 2.0)]);
    let before = current.entries.clone();

    let next = sync_block_stream(
        &current,
        "stream".into(),
        &[entry("note", 3.0), entry("cents", 4.0)],
    );

    assert!(next.is_none(), "nothing to set: the model already shows it");
    assert!(same_model(&current.entries, &before), "the model is kept");
    assert_eq!(current.entries.row_data(0).unwrap().value, 3.0);
    assert_eq!(current.entries.row_data(1).unwrap().value, 4.0);
}

#[test]
fn an_unchanged_silent_stream_sets_nothing() {
    let current = BlockStreamData {
        active: false,
        stream_kind: "stream".into(),
        entries: ModelRc::default(),
    };
    assert!(sync_block_stream(&current, "stream".into(), &[]).is_none());
}

#[test]
fn a_new_row_count_gets_a_new_model() {
    let current = shown("stream", &[entry("note", 1.0)]);
    let next = sync_block_stream(
        &current,
        "stream".into(),
        &[entry("a", 1.0), entry("b", 2.0)],
    )
    .expect("the row count moved");
    assert!(next.active);
    assert_eq!(next.entries.row_count(), 2);
}

#[test]
fn a_stream_that_goes_silent_turns_inactive() {
    let current = shown("stream", &[entry("note", 1.0)]);
    let next = sync_block_stream(&current, "stream".into(), &[]).expect("it went silent");
    assert!(!next.active);
    assert_eq!(next.entries.row_count(), 0);
}

#[test]
fn a_first_reading_turns_the_stream_active() {
    let next = sync_block_stream(
        &BlockStreamData::default(),
        "stream".into(),
        &[entry("note", 1.0)],
    )
    .expect("first reading");
    assert!(next.active);
    assert_eq!(next.stream_kind, "stream");
    assert_eq!(next.entries.row_count(), 1);
}
