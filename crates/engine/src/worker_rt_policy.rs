//! Responsibility: hands a realtime worker's scheduling policy to the threads that run its split paths.
//!
//! The engine spawns the threads that run a split's paths, but only the audio
//! backend knows how to put a thread in the realtime class. The backend
//! registers how (`set_promoter`) and tells each worker thread the policy it
//! runs at (`set_thread_policy`); a path thread then takes the policy of the
//! worker that drives its split.

use std::cell::Cell;
use std::sync::OnceLock;

/// Puts the calling thread in the realtime class: `(period_ns, computation_ns)`.
pub type Promoter = fn(u64, u64);

static PROMOTER: OnceLock<Promoter> = OnceLock::new();

thread_local! {
    static POLICY: Cell<Option<(u64, u64)>> = const { Cell::new(None) };
}

/// Register how a thread is promoted. The first registration wins.
pub fn set_promoter(promoter: Promoter) {
    let _ = PROMOTER.set(promoter);
}

/// `true` once a backend has said how to promote a thread.
pub fn promoter_installed() -> bool {
    PROMOTER.get().is_some()
}

/// Record the realtime policy the calling thread runs at.
pub fn set_thread_policy(period_ns: u64, computation_ns: u64) {
    POLICY.with(|policy| policy.set(Some((period_ns, computation_ns))));
}

/// The realtime policy the calling thread runs at, if it has one.
pub fn thread_policy() -> Option<(u64, u64)> {
    POLICY.with(Cell::get)
}

/// Put the calling thread at `policy`, as its worker is.
pub(crate) fn adopt(policy: (u64, u64)) {
    if let Some(promote) = PROMOTER.get() {
        promote(policy.0, policy.1);
    }
    set_thread_policy(policy.0, policy.1);
}
