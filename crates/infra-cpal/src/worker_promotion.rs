//! Responsibility: promotes a dsp worker to the realtime class its split-path threads inherit.

use crate::rt_thread_policy::promote_to_audio_rt;

/// Promote the calling worker thread to realtime at `(period_ns,
/// computation_ns)`; the threads that run its splits' paths take the same.
pub(crate) fn promote_worker(period_ns: u64, computation_ns: u64) {
    engine::worker_rt_policy::set_promoter(promote_to_audio_rt);
    promote_to_audio_rt(period_ns, computation_ns);
    engine::worker_rt_policy::set_thread_policy(period_ns, computation_ns);
}

#[cfg(test)]
#[path = "worker_promotion_tests.rs"]
mod tests;
