//! Tests for `lock_recover` — issue #415.
//!
//! The chain runtime Mutex used to be `.lock().expect("chain runtime
//! poisoned")` at three sites in `runtime_graph.rs`. A prior panic
//! anywhere holding that lock would poison it and the next UI-triggered
//! rebuild (Save in Chain Input Groups) would abort the process.
//!
//! These tests pin the recovery behaviour: a poisoned Mutex is logged
//! and the inner state is still returned, so a subsequent rebuild can
//! overwrite the inconsistent state with a fresh one.

use std::sync::{Arc, Mutex};

use crate::runtime_state::lock_recover;

#[test]
fn lock_recover_returns_guard_when_not_poisoned() {
    let m: Mutex<u32> = Mutex::new(42);
    let g = lock_recover(&m, "test");
    assert_eq!(*g, 42);
}

#[test]
fn lock_recover_returns_inner_when_poisoned_by_prior_panic() {
    let m: Arc<Mutex<u32>> = Arc::new(Mutex::new(7));

    // Poison the lock from another thread.
    let m_clone = Arc::clone(&m);
    let _ = std::thread::spawn(move || {
        let _guard = m_clone.lock().expect("not yet poisoned");
        panic!("intentional poison for test");
    })
    .join();

    // Sanity: the lock IS poisoned now.
    assert!(
        m.lock().is_err(),
        "lock must be poisoned before exercising lock_recover"
    );

    // The fix: lock_recover returns the guard anyway.
    let g = lock_recover(&m, "test");
    assert_eq!(
        *g, 7,
        "poisoned data is still accessible — only the poison flag was set"
    );
}

#[test]
fn lock_recover_lets_caller_overwrite_inconsistent_state() {
    // This mirrors what `update_chain_runtime_state` does at the swap
    // sites (Step 1 and Step 3): take a write lock, replace the entire
    // state. Even if a prior panic left the state inconsistent,
    // overwriting is safe.
    let m: Arc<Mutex<Vec<u32>>> = Arc::new(Mutex::new(vec![1, 2, 3]));

    let m_clone = Arc::clone(&m);
    let _ = std::thread::spawn(move || {
        let mut g = m_clone.lock().expect("not yet poisoned");
        g.clear(); // mid-write, leaving an "inconsistent" empty vec
        panic!("intentional poison mid-write");
    })
    .join();

    assert!(m.lock().is_err(), "must be poisoned for the regression");

    {
        let mut g = lock_recover(&m, "test");
        *g = vec![10, 20, 30]; // wholesale replacement, as the rebuild path does
    }

    let g = lock_recover(&m, "test");
    assert_eq!(*g, vec![10, 20, 30]);
}
