//! Responsibility: wires the process's private memory in use that is not wired yet.
//!
//! #980: on a machine out of memory the kernel compresses pages of OpenRig
//! that the DSP touches only every few seconds (a reverb walks its delay line
//! once per loop). The dsp-worker then stalls decompressing them — 1-4 ms
//! buffers at a third of their normal speed — and the output underruns.
//! Wired pages are never compressed or swapped.
//!
//! What gets wired: every private writable region something already touched
//! (resident or compressed pages), up to [`MAX_REGION`] each and a quarter of
//! the machine's RAM in total. Address space nobody touched stays as it is —
//! wiring a reservation would fault all of it into RAM. A region is wired
//! once: a later pass skips it. Runs off the audio threads; macOS only.

#[cfg(target_os = "macos")]
mod imp {
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
    /// A region larger than this is a reservation, not a working set.
    const MAX_REGION: u64 = 256 << 20;

    /// One region as the kernel reports it.
    struct Region {
        start: u64,
        size: u64,
        writable: bool,
        private: bool,
        touched: bool,
        wired: bool,
    }

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
        // `vm_region_basic_info_64`: word 8 low half is `user_wired_count`.
        let (mut basic_start, mut basic_size) = (start, 0u64);
        let mut basic = [0i32; 9];
        let mut basic_count = 9u32;
        let wired = unsafe {
            mach_vm_region(
                mach_task_self_,
                &mut basic_start,
                &mut basic_size,
                VM_REGION_BASIC_INFO_64,
                basic.as_mut_ptr(),
                &mut basic_count,
                &mut object,
            )
        } == 0
            && basic_start == start
            && basic[8] as u32 & 0xFFFF > 0;
        let share_mode = (extended[7] as u32 >> 24) as u8;
        Some(Region {
            start,
            size,
            writable: extended[0] & VM_PROT_WRITE != 0,
            private: PRIVATE_SHARE_MODES.contains(&share_mode),
            touched: extended[2] > 0 || extended[4] > 0,
            wired,
        })
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

    /// Wires every eligible region not wired yet; returns (regions, bytes)
    /// wired by this pass and the bytes wired in total.
    pub(crate) fn wire_private_memory() -> (usize, u64, u64) {
        let budget = wiring_budget();
        let (mut regions, mut bytes, mut total) = (0usize, 0u64, 0u64);
        let mut address = 0u64;
        while let Some(region) = region_at(address) {
            address = region.start + region.size;
            if !(region.writable && region.private && region.touched) {
                continue;
            }
            if region.wired {
                total += region.size;
                continue;
            }
            if region.size > MAX_REGION || total + region.size > budget {
                continue;
            }
            if unsafe { mlock(region.start as *const _, region.size as usize) } == 0 {
                regions += 1;
                bytes += region.size;
                total += region.size;
            }
        }
        (regions, bytes, total)
    }
}

/// One pass over the process's regions (see the module docs).
#[cfg(target_os = "macos")]
pub(crate) fn wire_private_memory() {
    let (regions, bytes, total) = imp::wire_private_memory();
    if regions > 0 {
        log::info!(
            "memory residency: wired {regions} regions ({} MB), {} MB resident for good",
            bytes >> 20,
            total >> 20
        );
    }
}

#[cfg(not(target_os = "macos"))]
pub(crate) fn wire_private_memory() {}

#[cfg(all(test, target_os = "macos"))]
#[path = "memory_wiring_tests.rs"]
pub(crate) mod memory_wiring_tests;
