//! #913 — the wait-free publish/read contract between a block and the GUI.
//!
//! The producer runs on a worker thread and the GUI reads on the event loop;
//! neither may block the other. What must hold: a fresh handle reads as empty
//! (the GUI draws before any block has published), a store is visible to a
//! reader that already holds the handle, and a reader holding an older snapshot
//! keeps reading it — publishing must never tear what someone is drawing.

use super::{new_stream_handle, StreamEntry};
use std::sync::Arc;

fn entry(key: &str, value: f32) -> StreamEntry {
    StreamEntry {
        key: key.to_string(),
        value,
        text: format!("{value}"),
        peak: 0.0,
    }
}

#[test]
fn a_fresh_handle_reads_as_empty() {
    let handle = new_stream_handle();
    assert!(handle.load().is_empty());
}

#[test]
fn a_published_snapshot_is_visible_to_a_reader_holding_the_same_handle() {
    let handle = new_stream_handle();
    let reader = Arc::clone(&handle);
    handle.store(Arc::new(vec![entry("level", 0.5)]));
    let seen = reader.load();
    assert_eq!(seen.len(), 1);
    assert_eq!(seen[0].key, "level");
    assert_eq!(seen[0].value, 0.5);
}

#[test]
fn a_snapshot_already_loaded_is_not_torn_by_the_next_publish() {
    let handle = new_stream_handle();
    handle.store(Arc::new(vec![entry("level", 0.25)]));
    let drawing = handle.load_full();
    handle.store(Arc::new(vec![entry("level", 0.9), entry("peak", 1.0)]));
    assert_eq!(
        drawing.len(),
        1,
        "the GUI keeps drawing the snapshot it took, whatever the worker publishes"
    );
    assert_eq!(drawing[0].value, 0.25);
    assert_eq!(handle.load().len(), 2, "the next read sees the new one");
}

#[test]
fn publishing_from_another_thread_reaches_the_reader() {
    let handle = new_stream_handle();
    let producer = Arc::clone(&handle);
    std::thread::spawn(move || {
        producer.store(Arc::new(vec![entry("from-worker", 1.0)]));
    })
    .join()
    .expect("worker thread");
    assert_eq!(handle.load()[0].key, "from-worker");
}
