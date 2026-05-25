//! Tests for the single-shot MIDI learn-mode flag (#513 / #493).

use super::learn::LearnState;
use std::sync::Arc;

#[test]
fn defaults_to_inactive() {
    let s = Arc::new(LearnState::default());
    assert!(!s.is_active());
}

#[test]
fn start_then_stop_returns_to_inactive() {
    let s = Arc::new(LearnState::default());
    s.start();
    assert!(s.is_active());
    s.stop();
    assert!(!s.is_active());
}

#[test]
fn capturing_one_event_auto_stops() {
    let s = Arc::new(LearnState::default());
    s.start();
    s.on_event_captured();
    assert!(!s.is_active(), "single-shot capture auto-stops");
}

#[test]
fn double_start_idempotent() {
    let s = Arc::new(LearnState::default());
    s.start();
    s.start();
    assert!(s.is_active());
    s.stop();
    assert!(!s.is_active());
}
