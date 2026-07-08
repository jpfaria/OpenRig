//! Main-thread deferral for VST3 teardown (issue #778).
//!
//! A VST3 plugin's `IComponent::terminate()` / `IEditController::terminate()`
//! tears down the plugin's native (JUCE/AppKit) editor components. On macOS
//! that MUST run on the main thread. But a superseded chain runtime — holding
//! the `Vst3Plugin` whose editor is open — is dropped on the `openrig-control-worker`
//! thread (issue #672), so the terminate ran off the main thread and crashed
//! (`~SliderLabelComp` → `makeKeyWindow` → EXC_BAD_ACCESS).
//!
//! This module lets `Vst3Plugin::drop` hand its teardown to the main thread:
//! the UI registers itself with [`mark_main_thread`] at startup and calls
//! [`drain_main_thread_deferred`] on its frontend tick. Off-main drops enqueue;
//! the drain runs them on the main thread. When no main thread is registered
//! (CLI, tests, headless render), teardown runs inline — those paths never open
//! a native editor, so there is nothing to marshal.

use std::sync::{Mutex, OnceLock};
use std::thread::ThreadId;

/// The thread the UI declared as its main/AppKit thread, if any.
static MAIN_THREAD: OnceLock<ThreadId> = OnceLock::new();

/// Teardown closures deferred from non-main threads, awaiting the drain.
type Deferred = Box<dyn FnOnce() + Send>;
static QUEUE: OnceLock<Mutex<Vec<Deferred>>> = OnceLock::new();

fn queue() -> &'static Mutex<Vec<Deferred>> {
    QUEUE.get_or_init(|| Mutex::new(Vec::new()))
}

/// Record the current thread as the main/AppKit thread. Call once, on the UI
/// thread, at startup. Subsequent calls are ignored (first wins).
pub fn mark_main_thread() {
    let _ = MAIN_THREAD.set(std::thread::current().id());
}

/// Whether a main thread was registered (i.e. a GUI is running).
fn main_thread_registered() -> bool {
    MAIN_THREAD.get().is_some()
}

/// Whether the caller is on the registered main thread.
fn on_main_thread() -> bool {
    MAIN_THREAD
        .get()
        .is_some_and(|id| *id == std::thread::current().id())
}

/// Run `teardown` now if it is already safe (on the main thread, or no GUI is
/// running), otherwise defer it to be run by [`drain_main_thread_deferred`] on
/// the main thread.
pub fn run_on_main_or_defer(teardown: Deferred) {
    if !main_thread_registered() || on_main_thread() {
        teardown();
    } else {
        queue().lock().expect("vst3 teardown queue poisoned").push(teardown);
    }
}

/// Run every deferred teardown. MUST be called on the main thread, from the
/// UI's frontend tick. No-op when nothing is pending.
pub fn drain_main_thread_deferred() {
    let pending: Vec<Deferred> = {
        let mut q = queue().lock().expect("vst3 teardown queue poisoned");
        if q.is_empty() {
            return;
        }
        std::mem::take(&mut *q)
    };
    for teardown in pending {
        teardown();
    }
}

#[cfg(test)]
#[path = "main_thread_tests.rs"]
mod tests;
