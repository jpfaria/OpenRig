//! #913 — clearing a latency badge once its ten seconds are up.
//!
//! The sweep runs every 500 ms over every open display window, so it must do
//! nothing at all in the common case: a row already reading zero is left alone,
//! because rewriting an unchanged row re-renders the whole chain list on every
//! tick.

use super::clear_expired_badges;
use crate::latency_probe::new_windows;
use crate::ProjectChainItem;
use slint::{Model, VecModel};
use std::rc::Rc;
use std::time::{Duration, Instant};

fn rows(latencies: &[f32]) -> Rc<VecModel<ProjectChainItem>> {
    Rc::new(VecModel::from(
        latencies
            .iter()
            .map(|ms| ProjectChainItem {
                latency_ms: *ms,
                ..Default::default()
            })
            .collect::<Vec<_>>(),
    ))
}

#[test]
fn a_window_that_has_not_closed_yet_keeps_its_badge() {
    let now = Instant::now();
    let chains = rows(&[4.2]);
    let windows = new_windows();
    windows
        .borrow_mut()
        .insert(0, now + Duration::from_secs(10));

    assert!(clear_expired_badges(&chains, &windows, now).is_empty());
    assert_eq!(chains.row_data(0).expect("row").latency_ms, 4.2);
    assert_eq!(windows.borrow().len(), 1, "the window stays open");
}

#[test]
fn a_closed_window_clears_its_badge_and_is_forgotten() {
    let now = Instant::now();
    let chains = rows(&[4.2]);
    let windows = new_windows();
    windows
        .borrow_mut()
        .insert(0, now - Duration::from_millis(1));

    assert_eq!(clear_expired_badges(&chains, &windows, now), vec![0]);
    assert_eq!(chains.row_data(0).expect("row").latency_ms, 0.0);
    assert!(windows.borrow().is_empty());
}

#[test]
fn a_window_closing_exactly_now_counts_as_closed() {
    let now = Instant::now();
    let chains = rows(&[1.0]);
    let windows = new_windows();
    windows.borrow_mut().insert(0, now);
    assert_eq!(clear_expired_badges(&chains, &windows, now), vec![0]);
}

#[test]
fn only_the_expired_chains_badge_is_cleared() {
    let now = Instant::now();
    let chains = rows(&[1.0, 2.0, 3.0]);
    let windows = new_windows();
    windows.borrow_mut().insert(1, now - Duration::from_secs(1));
    windows.borrow_mut().insert(2, now + Duration::from_secs(5));

    assert_eq!(clear_expired_badges(&chains, &windows, now), vec![1]);
    assert_eq!(chains.row_data(0).expect("row").latency_ms, 1.0);
    assert_eq!(chains.row_data(1).expect("row").latency_ms, 0.0);
    assert_eq!(chains.row_data(2).expect("row").latency_ms, 3.0);
}

#[test]
fn a_row_already_reading_zero_is_not_rewritten() {
    let now = Instant::now();
    let chains = rows(&[0.0]);
    let windows = new_windows();
    windows.borrow_mut().insert(0, now - Duration::from_secs(1));
    assert!(
        clear_expired_badges(&chains, &windows, now).is_empty(),
        "rewriting an unchanged row re-renders the chain list every 500 ms"
    );
    assert!(windows.borrow().is_empty(), "the window is still forgotten");
}

#[test]
fn an_expired_window_for_a_row_that_no_longer_exists_is_just_forgotten() {
    let now = Instant::now();
    let chains = rows(&[1.0]);
    let windows = new_windows();
    windows.borrow_mut().insert(7, now - Duration::from_secs(1));
    assert!(clear_expired_badges(&chains, &windows, now).is_empty());
    assert!(windows.borrow().is_empty());
}

#[test]
fn with_no_open_windows_the_sweep_does_nothing() {
    let chains = rows(&[1.0]);
    let windows = new_windows();
    assert!(clear_expired_badges(&chains, &windows, Instant::now()).is_empty());
    assert_eq!(chains.row_data(0).expect("row").latency_ms, 1.0);
}
