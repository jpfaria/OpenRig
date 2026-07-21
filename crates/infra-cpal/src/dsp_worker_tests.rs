//! Unit tests for dsp_worker (issue #792 split).
//! Two independent test modules kept out of the production file.

#[cfg(test)]
mod budget_tracker_tests {
    use super::super::BudgetTracker;

    const PERIOD_NS: u64 = 1_451_000; // 64 frames @ 44.1 kHz
    /// The real per-buffer DSP compute on an M4: microseconds. Far under any
    /// sane budget.
    const CHEAP_NS: u64 = 50_000;
    /// A transient PREEMPTION spike: the worker was descheduled mid-DSP, so the
    /// wall-clock `elapsed` reads multi-ms even though the COMPUTE was cheap.
    const PREEMPT_SPIKE_NS: u64 = 3_000_000;

    /// Count how many times `observe` re-declares the RT budget (returns Some)
    /// over a run of `n` buffers whose cost is `cost(i)`.
    fn count_redeclares(b: &mut BudgetTracker, n: usize, cost: impl Fn(usize) -> u64) -> usize {
        (0..n)
            .filter(|&i| b.observe(cost(i), PERIOD_NS).is_some())
            .count()
    }

    /// Issue (2026-06-17): the macOS RT dsp-worker stalled 2-3 ms intermittently
    /// on a SINGLE light chain on an idle M4 despite a successful RT promotion
    /// (rc=0), crackling under real load. Root cause A/B-confirmed on the real
    /// Scarlett (peak worker load ~9x → <1.6x with the re-budget disabled): the
    /// #698 adaptive BudgetTracker re-declares the RT time-constraint policy
    /// (`thread_policy_set`) on every buffer whose WALL-CLOCK cost spiked — but a
    /// wall-clock spike is PREEMPTION, not a real DSP cost increase. Each
    /// re-declaration is a syscall on the RT worker that perturbs its own
    /// scheduling → more stalls (a feedback loop).
    ///
    /// A steady, cheap workload with occasional transient preemption spikes must
    /// NOT churn the budget. (Deterministic, no hardware, no ear.)
    #[test]
    fn does_not_rebudget_on_transient_preemption_spikes() {
        let mut b = BudgetTracker::new(PERIOD_NS * 85 / 100);

        // Warm up to the real cheap cost. One legitimate down-declaration when
        // the first window closes is expected and fine.
        let _ = count_redeclares(&mut b, BudgetTracker::WINDOW as usize, |_| CHEAP_NS);

        // Steady cheap stream with a single transient preemption spike ONCE PER
        // window (one descheduled buffer every ~3 s of audio) — the realistic
        // shape: most buffers cheap, an occasional multi-ms preemption. The
        // workload did NOT get heavier; every spike is the worker being
        // descheduled, not real cost.
        let step = BudgetTracker::WINDOW as usize + 1;
        let redeclares = count_redeclares(&mut b, BudgetTracker::WINDOW as usize * 8, |i| {
            if i % step == 0 {
                PREEMPT_SPIKE_NS
            } else {
                CHEAP_NS
            }
        });

        assert!(
            redeclares <= 1,
            "BudgetTracker re-declared the RT budget {redeclares}x on a steady-cheap \
             workload with only transient preemption spikes. Each re-declaration is a \
             thread_policy_set on the RT worker that stalls it — the #698 single-chain \
             crackle. A transient wall-clock spike is preemption, not a real cost \
             increase, and must not re-budget."
        );
    }

    /// The other half of the contract: a GENUINE sustained cost increase (a
    /// block added, a real rebuild) MUST still re-declare promptly — the #698
    /// behaviour we keep. This guards against "fix the churn by never adapting".
    #[test]
    fn still_rebudgets_on_a_sustained_real_cost_increase() {
        let mut b = BudgetTracker::new(PERIOD_NS * 85 / 100);
        // Settle to the real cheap cost first. #743: the budget is sticky
        // downward, so it shrinks only after DOWN_SUSTAIN low windows — warm up
        // past that so `declared` actually reaches the cheap cost before the
        // increase (a 1-window warmup would leave it at the cold 85%).
        let warmup = BudgetTracker::WINDOW as usize * (BudgetTracker::DOWN_SUSTAIN as usize + 2);
        let _ = count_redeclares(&mut b, warmup, |_| CHEAP_NS);

        // Now the chain genuinely gets heavier and STAYS heavier (sustained,
        // not a one-off spike): ~60% of the period, every buffer.
        let heavy = PERIOD_NS * 60 / 100;
        let redeclares = count_redeclares(&mut b, BudgetTracker::WINDOW as usize, |_| heavy);
        assert!(
            redeclares >= 1,
            "a sustained real cost increase must re-declare the RT budget (the #698 \
             adaptation we keep), got {redeclares} re-declarations"
        );
    }

    /// A steady cheap workload re-declares the budget DOWN exactly once (to the
    /// real cost) and then SETTLES — the hysteresis must stop it re-declaring
    /// every window. (Re-declaration is a `thread_policy_set` syscall on the RT
    /// worker; needless ones perturb its scheduling.)
    #[test]
    fn steady_cheap_workload_settles_after_one_down_declaration() {
        let mut b = BudgetTracker::new(PERIOD_NS * 85 / 100);
        let redeclares = count_redeclares(&mut b, BudgetTracker::WINDOW as usize * 6, |_| CHEAP_NS);
        assert_eq!(
            redeclares, 1,
            "a steady cheap workload re-declares once (down to real cost) then settles"
        );
    }

    /// The RT budget must be measured in THREAD CPU TIME, not wall-clock, so a
    /// preemption stall (the worker descheduled mid-DSP) cannot be mistaken for
    /// DSP cost. This pins the property of `thread_cpu_time_ns`: a sleep (the
    /// thread NOT running — a stand-in for preemption) does NOT advance thread
    /// CPU time, while it fully advances wall-clock. Deterministic, no hardware.
    #[test]
    fn thread_cpu_time_excludes_preemption_sleep() {
        let Some(c0) = super::super::thread_cpu_time_ns() else {
            return; // platform without per-thread CPU clock — fallback path
        };
        let wall0 = std::time::Instant::now();

        // Stand-in for preemption: the thread is descheduled (sleeping) — not
        // computing — for 50 ms.
        std::thread::sleep(std::time::Duration::from_millis(50));
        // A little real compute so thread CPU time advances measurably.
        let mut acc = 0u64;
        for i in 0..2_000_000u64 {
            acc = acc.wrapping_add(i.rotate_left(7));
        }
        std::hint::black_box(acc);

        let compute_ns = super::super::thread_cpu_time_ns().unwrap() - c0;
        let wall_ns = wall0.elapsed().as_nanos() as u64;

        assert!(
            wall_ns >= 50_000_000,
            "wall-clock {wall_ns}ns must include the 50ms sleep"
        );
        assert!(
            compute_ns < 40_000_000,
            "thread CPU time {compute_ns}ns must EXCLUDE the 50ms preemption sleep — \
             this is why the RT budget is measured in compute time: wall-clock would \
             attribute a preemption stall as DSP cost and churn the budget."
        );
    }
}

#[cfg(test)]
mod saturation_recovery_tests {
    use super::super::SaturationRecovery;

    /// Recovery (re-promote + drop backlog, #670 death-spiral break) must fire
    /// ONLY after `threshold` CONSECUTIVE saturated drains, then reset — never
    /// on a transient saturation that the ring recovers from on its own.
    #[test]
    fn fires_only_after_threshold_consecutive_saturations() {
        let mut r = SaturationRecovery::new(3);
        assert!(!r.observe(true), "1st saturation: not yet");
        assert!(!r.observe(true), "2nd saturation: not yet");
        assert!(r.observe(true), "3rd consecutive saturation: recover NOW");
        // After firing it restarts the run.
        assert!(!r.observe(true), "run restarted after firing");
        assert!(!r.observe(true));
        assert!(r.observe(true), "next 3-run fires again");
    }

    #[test]
    fn a_single_healthy_drain_resets_the_run() {
        let mut r = SaturationRecovery::new(3);
        assert!(!r.observe(true));
        assert!(!r.observe(true));
        // A healthy (non-saturated) drain breaks the streak — the spiral
        // resolved on its own, so recovery must NOT fire.
        assert!(!r.observe(false), "healthy drain resets, no recovery");
        assert!(!r.observe(true), "streak restarts from zero");
        assert!(!r.observe(true));
        assert!(r.observe(true), "needs a fresh full streak to fire");
    }

    #[test]
    fn threshold_one_fires_on_every_saturation() {
        let mut r = SaturationRecovery::new(1);
        assert!(r.observe(true));
        assert!(r.observe(true));
        assert!(!r.observe(false));
    }
}
