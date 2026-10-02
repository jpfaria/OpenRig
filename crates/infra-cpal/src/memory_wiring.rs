//! Responsibility: wires the audio's memory in use that is not wired yet.
//!
//! On a machine out of memory the kernel compresses pages of OpenRig that the
//! DSP touches only every few seconds (a reverb walks its delay line once per
//! loop). The dsp-worker then stalls decompressing them — 1-4 ms buffers at a
//! third of their normal speed — and the output underruns. Wired pages are
//! never compressed or swapped.
//!
//! Only the audio's memory is wired: the audio zone's regions
//! (`engine::audio_zone_regions`). Wired pages stay resident even once
//! freed, so wiring the whole process kept every byte the app ever used — the
//! UI, a project load, a rebuild's scratch — in RAM for good. Where the audio
//! zone cannot exist (the router refused) the pass falls back to the whole
//! process.
//!
//! This file is the macOS side: it reads the regions from the kernel and
//! wires with `mlock`. What a pass wires — private writable
//! regions something already touched, up to 256 MB each and a quarter of the
//! machine's RAM in total, each region once — is decided in
//! `memory_wiring_pass`; what it left unwired is logged
//! (`memory_wiring_report`). Passes run one at a time. Runs off the audio
//! threads; macOS only.

#[cfg(target_os = "macos")]
mod imp {
    use crate::memory_wiring_pass::{run_pass, Region, Report};
    use crate::memory_wiring_report::report_lines;

    extern "C" {
        static mach_task_self_: u32;
        fn mach_vm_region(
            task: u32,
            address: *mut u64,
            size: *mut u64,
            flavor: i32,
            info: *mut i32,
            count: *mut u32,
            object_name: *mut u32,
        ) -> i32;
        fn mlock(addr: *const std::ffi::c_void, len: usize) -> i32;
        fn sysctlbyname(
            name: *const std::ffi::c_char,
            old: *mut std::ffi::c_void,
            old_len: *mut usize,
            new: *mut std::ffi::c_void,
            new_len: usize,
        ) -> i32;
    }

    const VM_REGION_BASIC_INFO_64: i32 = 9;
    const VM_REGION_EXTENDED_INFO: i32 = 13;
    const VM_PROT_WRITE: i32 = 2;
    /// `share_mode` values of `vm_region_extended_info` that are this
    /// process's own memory: copy-on-write, private, private aliased.
    const PRIVATE_SHARE_MODES: [u8; 3] = [1, 2, 6];

    /// The region at or after `address`, or `None` past the last one.
    fn region_at(address: u64) -> Option<Region> {
        let (mut start, mut size, mut object) = (address, 0u64, 0u32);
        // `vm_region_extended_info`: protection, user_tag, pages_resident,
        // pages_shared_now_private, pages_swapped_out, pages_dirtied,
        // ref_count, {shadow_depth: u16, external_pager: u8, share_mode: u8}.
        let mut extended = [0i32; 9];
        let mut count = 9u32;
        let kr = unsafe {
            mach_vm_region(
                mach_task_self_,
                &mut start,
                &mut size,
                VM_REGION_EXTENDED_INFO,
                extended.as_mut_ptr(),
                &mut count,
                &mut object,
            )
        };
        if kr != 0 {
            return None;
        }
        let share_mode = (extended[7] as u32 >> 24) as u8;
        Some(Region {
            start,
            size,
            writable: extended[0] & VM_PROT_WRITE != 0,
            private: PRIVATE_SHARE_MODES.contains(&share_mode),
            touched: extended[2] > 0 || extended[4] > 0,
            wired: is_wired(start),
        })
    }

    /// Whether the region starting at `start` carries a user wire now.
    fn is_wired(start: u64) -> bool {
        // `vm_region_basic_info_64`: word 8 low half is `user_wired_count`.
        let (mut basic_start, mut basic_size, mut object) = (start, 0u64, 0u32);
        let mut basic = [0i32; 9];
        let mut basic_count = 9u32;
        let kr = unsafe {
            mach_vm_region(
                mach_task_self_,
                &mut basic_start,
                &mut basic_size,
                VM_REGION_BASIC_INFO_64,
                basic.as_mut_ptr(),
                &mut basic_count,
                &mut object,
            )
        };
        kr == 0 && basic_start == start && basic[8] as u32 & 0xFFFF > 0
    }

    /// The regions of the process between `start` and `end`, in address
    /// order, each clipped to that span.
    fn regions_within(start: u64, end: u64) -> Vec<Region> {
        let mut regions = Vec::new();
        let mut address = start;
        while address < end {
            let Some(region) = region_at(address) else {
                break;
            };
            if region.start >= end {
                break;
            }
            let from = region.start.max(start);
            let to = (region.start + region.size).min(end);
            regions.push(Region {
                start: from,
                size: to - from,
                wired: is_wired(from),
                ..region
            });
            address = to;
        }
        regions
    }

    /// The audio zone's regions, or every region of the process when the
    /// audio's memory cannot be told apart.
    fn regions() -> Vec<Region> {
        if !engine::audio_zone_router::install() {
            return regions_within(0, u64::MAX);
        }
        match engine::audio_zone_regions::audio_zone_ranges() {
            Some(ranges) => ranges
                .iter()
                .flat_map(|&(start, size)| regions_within(start, start + size))
                .collect(),
            None => regions_within(0, u64::MAX),
        }
    }

    /// A quarter of the machine's RAM: the most this ever wires.
    fn wiring_budget() -> u64 {
        let mut ram = 0u64;
        let mut len = std::mem::size_of::<u64>();
        let rc = unsafe {
            sysctlbyname(
                c"hw.memsize".as_ptr(),
                &mut ram as *mut u64 as *mut std::ffi::c_void,
                &mut len,
                std::ptr::null_mut(),
                0,
            )
        };
        if rc == 0 {
            ram / 4
        } else {
            1 << 30
        }
    }

    /// The previous pass's report. Its lock also runs passes one at a time:
    /// between a region's last check and its `mlock` no other pass may wire
    /// it (concurrent passes once stacked eleven wires on one region).
    /// Ordinary threads only, never audio.
    static LAST_PASS: std::sync::Mutex<Option<Report>> = std::sync::Mutex::new(None);

    /// One pass over the process's regions against the machine's budget;
    /// returns what to log about it.
    pub(crate) fn wire_private_memory() -> Vec<(log::Level, String)> {
        let mut last = LAST_PASS
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner);
        let report = run_pass(
            &regions(),
            wiring_budget(),
            |region| !is_wired(region.start),
            |region| unsafe { mlock(region.start as *const _, region.size as usize) } == 0,
        );
        let lines = report_lines(&report, last.as_ref());
        *last = Some(report);
        lines
    }
}

/// One pass over the process's regions (see the module docs).
#[cfg(target_os = "macos")]
pub(crate) fn wire_private_memory() {
    for (level, line) in imp::wire_private_memory() {
        log::log!(level, "{line}");
    }
}

#[cfg(not(target_os = "macos"))]
pub(crate) fn wire_private_memory() {}

#[cfg(all(test, target_os = "macos"))]
#[path = "memory_wiring_tests.rs"]
pub(crate) mod memory_wiring_tests;
