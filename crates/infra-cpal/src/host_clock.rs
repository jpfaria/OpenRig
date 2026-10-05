//! Responsibility: reads the host clock the audio streams timestamp against.
//!
//! On macOS this is the clock cpal stamps every callback with (mach absolute
//! time in nanoseconds), so an instant read here and a callback's capture or
//! playback instant are directly comparable. Elsewhere the callback clock is
//! not the same, so the stream instants are not used for timing.

#[cfg(target_os = "macos")]
pub(crate) const MATCHES_STREAM_CLOCK: bool = true;
#[cfg(not(target_os = "macos"))]
pub(crate) const MATCHES_STREAM_CLOCK: bool = false;

/// Nanoseconds on the host clock.
#[cfg(target_os = "macos")]
pub(crate) fn now_ns() -> u64 {
    use std::sync::OnceLock;

    #[repr(C)]
    struct Timebase {
        numer: u32,
        denom: u32,
    }
    extern "C" {
        fn mach_absolute_time() -> u64;
        fn mach_timebase_info(info: *mut Timebase) -> i32;
    }
    static TIMEBASE: OnceLock<(u32, u32)> = OnceLock::new();
    let (numer, denom) = *TIMEBASE.get_or_init(|| {
        let mut tb = Timebase { numer: 0, denom: 0 };
        // SAFETY: `tb` is a valid, writable `mach_timebase_info_data_t`.
        if unsafe { mach_timebase_info(&mut tb) } != 0 || tb.denom == 0 {
            (1, 1)
        } else {
            (tb.numer, tb.denom)
        }
    });
    // SAFETY: no arguments, no preconditions.
    let ticks = unsafe { mach_absolute_time() };
    (ticks as u128 * numer as u128 / denom as u128) as u64
}

/// Nanoseconds on a monotonic clock (counted from its first read, plus one so
/// it is never 0, which means "unknown").
#[cfg(not(target_os = "macos"))]
pub(crate) fn now_ns() -> u64 {
    use std::sync::OnceLock;
    use std::time::Instant;

    static EPOCH: OnceLock<Instant> = OnceLock::new();
    EPOCH.get_or_init(Instant::now).elapsed().as_nanos() as u64 + 1
}

/// When the first frame of an input buffer was captured; 0 when unknown.
#[cfg(not(all(target_os = "linux", feature = "jack")))]
pub(crate) fn capture_ns(info: &cpal::InputCallbackInfo) -> u64 {
    if !MATCHES_STREAM_CLOCK {
        return 0;
    }
    info.timestamp()
        .capture
        .checked_duration_since(cpal::StreamInstant::new(0, 0))
        .map_or(0, |since_boot| since_boot.as_nanos() as u64)
}

/// When the first frame of an output buffer will be heard, if known.
#[cfg(not(all(target_os = "linux", feature = "jack")))]
pub(crate) fn playback_ns(info: &cpal::OutputCallbackInfo) -> Option<u64> {
    if !MATCHES_STREAM_CLOCK {
        return None;
    }
    info.timestamp()
        .playback
        .checked_duration_since(cpal::StreamInstant::new(0, 0))
        .map(|since_boot| since_boot.as_nanos() as u64)
}

#[cfg(test)]
#[path = "host_clock_tests.rs"]
mod tests;
