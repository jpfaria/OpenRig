//! Responsibility: decides when a saturated worker may run full again.

/// Saturation-recovery policy (issue #670). Owner-hit failure mode: a one-off
/// multi-ms stall builds backlog; the worker then runs chronically over its
/// declared RT computation budget, the kernel demotes it (to an E core), and
/// EVERY buffer becomes multi-ms — the ring pins at its overflow clamp and the
/// chain never heals. The policy: after `threshold` CONSECUTIVE saturated
/// drains, demand recovery — the worker re-asserts its realtime promotion and
/// drops the backlog to bound latency. A single healthy drain resets the run.
pub(crate) struct SaturationRecovery {
    threshold: u32,
    run: u32,
}

impl SaturationRecovery {
    pub(crate) fn new(threshold: u32) -> Self {
        Self { threshold, run: 0 }
    }

    /// Record one drain; `saturated` = the backlog hit the overflow clamp.
    /// Returns `true` when recovery must run NOW (and restarts the counter).
    pub(crate) fn observe(&mut self, saturated: bool) -> bool {
        if !saturated {
            self.run = 0;
            return false;
        }
        self.run += 1;
        if self.run >= self.threshold {
            self.run = 0;
            return true;
        }
        false
    }
}
