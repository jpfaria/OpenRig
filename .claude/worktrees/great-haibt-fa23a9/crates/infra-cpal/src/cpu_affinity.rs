//! Linux CPU pinning helpers for the JACK RT callback and the DSP worker
//! threads.
//!
//! The JACK process callback runs on a thread libjack spawns inside our
//! process. Without explicit pinning it inherits the systemd
//! `CPUAffinity=` mask (little A55 cores on RK3588 in our deployment),
//! which makes it compete with the Slint UI thread for CPU. The DSP
//! worker thread we spawn ourselves is in the same boat. Both call
//! `detect_big_cores` to learn the highest-frequency cores by reading
//! sysfs, and `pin_thread_to_cpus` to apply the resulting mask to the
//! calling thread via `sched_setaffinity`.
//!
//! `#[inline]` is mandatory on every function here. `pin_thread_to_cpus`
//! and `detect_big_cores` are called from `JackProcessHandler::process`
//! on the first callback after the RT thread starts up — the audio
//! thread, by the textual-move contract from issue #194 Phase 5, must
//! not pay any extra call/jump for refactor reasons.

#![cfg(all(target_os = "linux", feature = "jack"))]

/// Pin the calling thread to the given CPU cores (Linux only).
#[inline]
pub(crate) fn pin_thread_to_cpus(cpus: &[usize]) {
    use std::mem;
    unsafe {
        let mut set: libc::cpu_set_t = mem::zeroed();
        for &cpu in cpus {
            libc::CPU_SET(cpu, &mut set);
        }
        let ret = libc::sched_setaffinity(0, mem::size_of::<libc::cpu_set_t>(), &set);
        if ret != 0 {
            log::warn!(
                "sched_setaffinity failed: {}",
                std::io::Error::last_os_error()
            );
        }
    }
}

/// Detect big cores on ARM big.LITTLE by reading max frequency from sysfs.
/// Returns CPU indices sorted by max frequency (highest first).
/// Falls back to CPUs 4-7 if sysfs is unavailable.
#[inline]
pub(crate) fn detect_big_cores() -> Vec<usize> {
    let mut cpu_freqs: Vec<(usize, u64)> = Vec::new();
    for cpu in 0..16 {
        let path = format!(
            "/sys/devices/system/cpu/cpu{}/cpufreq/cpuinfo_max_freq",
            cpu
        );
        if let Ok(contents) = std::fs::read_to_string(&path) {
            if let Ok(freq) = contents.trim().parse::<u64>() {
                cpu_freqs.push((cpu, freq));
            }
        }
    }
    if cpu_freqs.is_empty() {
        log::info!("DSP worker: sysfs unavailable, defaulting to CPUs 4-7");
        return vec![4, 5, 6, 7];
    }
    let max_freq = cpu_freqs.iter().map(|(_, f)| *f).max().unwrap_or(0);
    let big: Vec<usize> = cpu_freqs
        .iter()
        .filter(|(_, f)| *f == max_freq)
        .map(|(cpu, _)| *cpu)
        .collect();
    log::info!(
        "DSP worker: detected big cores {:?} (max_freq={}kHz)",
        big,
        max_freq
    );
    big
}
