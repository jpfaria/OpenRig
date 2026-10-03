//! Responsibility: lists the address ranges the audio zone holds.
//!
//! The memory wiring asks for these ranges to wire only the audio's memory:
//! the pages under the zone's live allocations, merged into page-aligned
//! spans (the zone does not report its segments as regions). They come from
//! the zone's own enumerator, run in-process with the zone
//! locked so no audio-thread allocation reshapes it mid-walk. Nothing may
//! allocate while the lock is held — a `free` routed to a locked zone would
//! deadlock — so ranges land in a buffer reserved beforehand, and a walk that
//! overflows it is redone with a bigger one. macOS only; `None` elsewhere or
//! while the router is not installed.

/// `MALLOC_PTR_IN_USE_RANGE_TYPE`: each live allocation.
#[cfg(target_os = "macos")]
const IN_USE: u32 = 1;

#[cfg(target_os = "macos")]
mod imp {
    use std::ffi::c_void;

    use crate::audio_zone_router::audio_zone_address;

    #[repr(C)]
    struct Range {
        address: usize,
        size: usize,
    }

    /// Where the enumerator's recorder writes: never grows while it runs.
    struct Sink {
        ranges: Vec<(u64, u64)>,
        overflowed: bool,
    }

    /// The zone lives in this process: an address reads as itself.
    unsafe extern "C" fn read_in_process(
        _task: u32,
        address: usize,
        _size: usize,
        out: *mut *mut c_void,
    ) -> i32 {
        *out = address as *mut c_void;
        0
    }

    unsafe extern "C" fn record(
        _task: u32,
        context: *mut c_void,
        _type: u32,
        ranges: *mut Range,
        count: u32,
    ) {
        let sink = &mut *(context as *mut Sink);
        for i in 0..count as usize {
            let range = &*ranges.add(i);
            if sink.ranges.len() == sink.ranges.capacity() {
                sink.overflowed = true;
                return;
            }
            sink.ranges.push((range.address as u64, range.size as u64));
        }
    }

    enum Walk {
        Done(Vec<(u64, u64)>),
        Overflowed,
        Failed,
    }

    extern "C" {
        static mach_task_self_: u32;
        static vm_page_size: usize;
    }

    /// `ranges` widened to whole pages, sorted, overlapping and touching
    /// spans merged.
    pub(super) fn page_spans(mut ranges: Vec<(u64, u64)>) -> Vec<(u64, u64)> {
        let page = unsafe { vm_page_size } as u64;
        ranges.sort_unstable();
        let mut spans: Vec<(u64, u64)> = Vec::new();
        for (address, size) in ranges {
            let start = address / page * page;
            let end = (address + size).div_ceil(page) * page;
            match spans.last_mut() {
                Some((last_start, last_size)) if start <= *last_start + *last_size => {
                    *last_size = (*last_start + *last_size).max(end) - *last_start;
                }
                _ => spans.push((start, end - start)),
            }
        }
        spans
    }

    /// One walk of the zone into a buffer of `capacity` ranges.
    unsafe fn walk(
        zone: *mut crate::audio_zone_router::MallocZone,
        type_mask: u32,
        capacity: usize,
    ) -> Walk {
        let introspect = &*(*zone).introspect;
        let Some(enumerate) = introspect.enumerator else {
            return Walk::Failed;
        };
        let mut sink = Sink {
            ranges: Vec::with_capacity(capacity),
            overflowed: false,
        };
        if let Some(lock) = introspect.force_lock {
            lock(zone);
        }
        let kr = enumerate(
            mach_task_self_,
            &mut sink as *mut Sink as *mut c_void,
            type_mask,
            zone as usize,
            read_in_process as *const c_void,
            record as *const c_void,
        );
        if let Some(unlock) = introspect.force_unlock {
            unlock(zone);
        }
        match (kr, sink.overflowed) {
            (0, false) => Walk::Done(sink.ranges),
            (0, true) => Walk::Overflowed,
            _ => Walk::Failed,
        }
    }

    pub(super) fn ranges(type_mask: u32) -> Option<Vec<(u64, u64)>> {
        let zone = audio_zone_address()?;
        let mut capacity = 1 << 10;
        while capacity <= 1 << 20 {
            match unsafe { walk(zone, type_mask, capacity) } {
                Walk::Done(ranges) => return Some(ranges),
                Walk::Overflowed => capacity *= 4,
                Walk::Failed => return None,
            }
        }
        None
    }
}

/// The pages under the audio zone's live allocations as page-aligned
/// `(start, size)` spans in address order, or `None` when the audio's memory
/// cannot be told apart (router not installed).
#[cfg(target_os = "macos")]
pub fn audio_zone_ranges() -> Option<Vec<(u64, u64)>> {
    imp::ranges(IN_USE).map(imp::page_spans)
}

/// Elsewhere there is no audio zone.
#[cfg(not(target_os = "macos"))]
pub fn audio_zone_ranges() -> Option<Vec<(u64, u64)>> {
    None
}

/// Bytes allocated in the audio zone now; 0 when it does not exist.
#[cfg(target_os = "macos")]
pub fn audio_zone_bytes_in_use() -> usize {
    imp::ranges(IN_USE).map_or(0, |ranges| {
        ranges.iter().map(|(_, size)| *size as usize).sum()
    })
}

/// Elsewhere there is no audio zone.
#[cfg(not(target_os = "macos"))]
pub fn audio_zone_bytes_in_use() -> usize {
    0
}
