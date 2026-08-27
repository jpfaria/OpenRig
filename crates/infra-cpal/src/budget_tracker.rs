//! Responsibility: tracks how much of its deadline the worker is spending.

/// Issue #698 — adaptive RT computation budget. Buffers measured per
/// window; at each window boundary the worker re-declares its
/// time-constraint computation from the measured worst case plus
/// headroom, so N concurrent chains together stay inside what the
/// kernel's admission will schedule. Plain data, worker-thread only.
pub(crate) struct BudgetTracker {
    /// Highest and second-highest measured cost in the current window. The
    /// budget is driven by the SECOND highest, so a single transient outlier
    /// (a preemption stall — the worker descheduled mid-DSP, which inflates
    /// the WALL-CLOCK measurement without being real compute cost) cannot move
    /// the budget. A genuine sustained cost increase shows up in the second
    /// highest too.
    pub(crate) window_max_ns: u64,
    pub(crate) window_2nd_ns: u64,
    pub(crate) window_count: u32,
    pub(crate) declared_ns: u64,
    /// Consecutive buffers measured over the declared budget. The fast
    /// up-correction fires only when this is SUSTAINED, never on a lone spike.
    pub(crate) consecutive_over: u32,
    /// #743: consecutive WINDOWS whose target sits below the declared budget,
    /// and the highest such target seen across that run. The budget shrinks only
    /// after a sustained run (never on a single low window), and settles to the
    /// run's high-water — so a steady but spiky load whose per-window cost
    /// alternates does not bounce the policy down and back up.
    pub(crate) low_run: u32,
    pub(crate) low_run_peak_ns: u64,
}

impl BudgetTracker {
    /// ≈3 s of buffers at 64 frames / 44.1 kHz between re-declarations.
    pub(crate) const WINDOW: u32 = 2048;
    /// A genuine cost increase persists; a preemption stall is one isolated
    /// buffer. Require this many CONSECUTIVE over-budget buffers before the
    /// fast up-correction re-declares — so a transient stall does not churn
    /// the RT policy (each re-declaration is a `thread_policy_set` syscall on
    /// the worker that itself perturbs its scheduling → more stalls).
    pub(crate) const SUSTAIN: u32 = 3;

    /// An idle/paused window measures (near) nothing — a drained chain's worker
    /// only copies the buffer and short-circuits, ~1-2 µs. Below 1% of the period
    /// the window carries no real cost signal, so the budget is left untouched;
    /// collapsing it to the floor only forces a fast-up re-declare the instant
    /// work resumes (#743). The threshold sits FAR under any measurable real
    /// workload (e.g. the #698 cheap-settle test's 50 µs ≈ 3.4% of the period),
    /// so genuine cheap chains still settle their budget down.
    pub(crate) const IDLE_NS_DIVISOR: u64 = 100;

    /// Windows of sustained below-budget cost required before the budget shrinks
    /// (≈11 s at 2048 buffers / 64 frames / 48 kHz). A shorter run is just
    /// normal window-to-window variance and must not re-declare (#743).
    pub(crate) const DOWN_SUSTAIN: u32 = 4;

    pub(crate) fn new(declared_ns: u64) -> Self {
        Self {
            window_max_ns: 0,
            window_2nd_ns: 0,
            window_count: 0,
            declared_ns,
            consecutive_over: 0,
            low_run: 0,
            low_run_peak_ns: 0,
        }
    }

    /// Record one processed buffer's cost; returns the new computation
    /// budget when the window closes AND it differs meaningfully from the
    /// declared one (hysteresis: 10% of the period).
    ///
    /// Fast up-correction: a SUSTAINED run of buffers over the declared budget
    /// means the chain genuinely got heavier (live rebuild, block added) and
    /// the kernel will demote an over-budget RT thread — re-declare the
    /// conservative 85% immediately instead of waiting for the window
    /// (measured: 128 underruns in the post-rebuild stretch of
    /// `rebuild_while_playing_keeps_the_cushion` without this). A SINGLE
    /// over-budget buffer is a preemption stall, not a cost change, and is
    /// ignored — otherwise the policy churns (the #698 single-chain crackle).
    pub(crate) fn observe(&mut self, elapsed_ns: u64, period_ns: u64) -> Option<u64> {
        if elapsed_ns > self.declared_ns {
            self.consecutive_over += 1;
        } else {
            self.consecutive_over = 0;
        }
        if self.consecutive_over >= Self::SUSTAIN && self.declared_ns < period_ns * 85 / 100 {
            self.consecutive_over = 0;
            return Some(self.reset(period_ns));
        }

        // Track the top two costs of the window.
        if elapsed_ns > self.window_max_ns {
            self.window_2nd_ns = self.window_max_ns;
            self.window_max_ns = elapsed_ns;
        } else if elapsed_ns > self.window_2nd_ns {
            self.window_2nd_ns = elapsed_ns;
        }
        self.window_count += 1;
        if self.window_count < Self::WINDOW {
            return None;
        }
        // Second-highest cost + 25% headroom, floored at 10% of the period (an
        // undersized budget also demotes — the reverted #670 attempt) and
        // capped at the validated 85%. Using the SECOND highest makes one
        // isolated preemption stall per window invisible to the budget.
        let robust = self.window_2nd_ns;
        self.window_max_ns = 0;
        self.window_2nd_ns = 0;
        self.window_count = 0;
        // #743: an idle/paused window (the worker measured ~nothing — a drained
        // chain) must NOT collapse the budget to the floor. Doing so only forces
        // the fast-up to re-declare the policy the instant work resumes, so every
        // pause/resume churns two `thread_policy_set` syscalls and the resulting
        // scheduling perturbation shows up as 4-6 ms late buffers. Keep the
        // standing budget across an idle window.
        if robust < period_ns / Self::IDLE_NS_DIVISOR {
            return None;
        }
        let target = (robust + robust / 4).clamp(period_ns / 10, period_ns * 85 / 100);
        let hyst = period_ns / 10;
        // Grow promptly: an under-budget RT thread gets demoted (#698 safety).
        if target > self.declared_ns + hyst {
            self.declared_ns = target;
            self.low_run = 0;
            self.low_run_peak_ns = 0;
            return Some(target);
        }
        // #743: the target sits meaningfully BELOW the standing budget. A single
        // low window is just variance — shrinking now only invites a re-grow next
        // window (the owner's 372 ↔ 592 µs steady-play churn). Shrink only after
        // a sustained run of low windows, and then to the run's HIGH-WATER so a
        // spiky-but-steady load settles to its peak instead of bouncing.
        if self.declared_ns > target + hyst {
            self.low_run += 1;
            self.low_run_peak_ns = self.low_run_peak_ns.max(target);
            if self.low_run >= Self::DOWN_SUSTAIN {
                let settled = self.low_run_peak_ns;
                self.declared_ns = settled;
                self.low_run = 0;
                self.low_run_peak_ns = 0;
                return Some(settled);
            }
            return None;
        }
        // Within hysteresis — the budget already fits the load.
        self.low_run = 0;
        self.low_run_peak_ns = 0;
        None
    }

    /// After a saturation spiral the chain's cost is unknown again —
    /// restart from the conservative cold-start budget.
    pub(crate) fn reset(&mut self, period_ns: u64) -> u64 {
        self.window_max_ns = 0;
        self.window_2nd_ns = 0;
        self.window_count = 0;
        self.consecutive_over = 0;
        self.declared_ns = period_ns * 85 / 100;
        self.declared_ns
    }
}
