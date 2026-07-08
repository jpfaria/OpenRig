//! Issue #778: a VST3 teardown enqueued from a non-main thread must NOT run
//! inline (that is the off-main `terminate()` that crashed) — it runs only when
//! the main thread drains. This is the deterministic guard for that marshaling;
//! the real crash only reproduces through the full app editor flow.

use super::*;
use std::sync::atomic::{AtomicUsize, Ordering};
use std::sync::Arc;

#[test]
fn off_main_teardown_defers_and_runs_only_on_drain() {
    // This test thread is the "main" (AppKit) thread. Only this test touches the
    // process-global registry, so first-wins picks this thread.
    mark_main_thread();

    let ran = Arc::new(AtomicUsize::new(0));

    // Enqueue a teardown FROM a non-main thread.
    let r = ran.clone();
    std::thread::spawn(move || {
        run_on_main_or_defer(Box::new(move || {
            r.fetch_add(1, Ordering::SeqCst);
        }));
    })
    .join()
    .unwrap();

    assert_eq!(
        ran.load(Ordering::SeqCst),
        0,
        "a teardown from a non-main thread must be deferred, not run inline"
    );

    drain_main_thread_deferred();

    assert_eq!(
        ran.load(Ordering::SeqCst),
        1,
        "draining on the main thread must run the deferred teardown exactly once"
    );

    // On the main thread it runs inline (no drain needed).
    let r2 = ran.clone();
    run_on_main_or_defer(Box::new(move || {
        r2.fetch_add(1, Ordering::SeqCst);
    }));
    assert_eq!(
        ran.load(Ordering::SeqCst),
        2,
        "a teardown on the main thread runs inline"
    );
}
