//! Issue #980 — the memory the audio touches must stay resident.
//!
//! Measured on the owner's rig: with the machine out of memory (swap full,
//! other builds running) the kernel compresses OpenRig pages the DSP touches
//! only every few seconds — the reverbs' delay lines — and the dsp-worker
//! stalls decompressing them: 1-4 ms buffers at IPC 0.6, every late buffer on
//! a burst of page faults, 1152..50816 frames lost per 90 s run. Wiring the
//! process's private writable memory took it to 0 in 6/6 runs.
//!
//! These pin the wiring itself: a buffer the audio side allocated and touched
//! is wired after a scan, wired once (a later scan does not stack another
//! wire on it), and address space nobody touched is never wired (a reserved
//! range would otherwise be faulted in whole). macOS only — the kernel
//! compressor is what this answers.

use super::wire_private_memory;

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
    fn mmap(
        addr: *mut std::ffi::c_void,
        len: usize,
        prot: i32,
        flags: i32,
        fd: i32,
        offset: i64,
    ) -> *mut std::ffi::c_void;
    fn munmap(addr: *mut std::ffi::c_void, len: usize) -> i32;
}

const VM_REGION_BASIC_INFO_64: i32 = 9;
const PROT_READ: i32 = 1;
const PROT_WRITE: i32 = 2;
const MAP_PRIVATE: i32 = 0x0002;
const MAP_ANON: i32 = 0x1000;
/// `VM_MAKE_TAG(250)`: an application tag of its own, so the mapping never
/// coalesces with a neighbouring region.
const APP_TAG_250: i32 = 250 << 24;

/// How many user wires the region holding `ptr` carries.
pub(crate) fn user_wired_count(ptr: *const u8) -> u16 {
    let mut address = ptr as u64;
    let mut size = 0u64;
    let mut info = [0i32; 9];
    let mut count = 9u32;
    let mut object = 0u32;
    let kr = unsafe {
        mach_vm_region(
            mach_task_self_,
            &mut address,
            &mut size,
            VM_REGION_BASIC_INFO_64,
            info.as_mut_ptr(),
            &mut count,
            &mut object,
        )
    };
    assert_eq!(kr, 0, "mach_vm_region failed");
    assert!(
        address <= ptr as u64 && (ptr as u64) < address + size,
        "no region holds the probe"
    );
    // `user_wired_count` is the low half of word 8 of `vm_region_basic_info_64`.
    (info[8] as u32 & 0xFFFF) as u16
}

/// A delay line the size of a reverb's: allocated for the audio, then
/// written once through.
pub(crate) fn touched_buffer() -> Vec<f32> {
    let mut buffer = {
        let _audio = engine::audio_alloc_scope::audio_allocations();
        vec![0.0_f32; 2 << 20]
    };
    for (i, sample) in buffer.iter_mut().enumerate().step_by(1024) {
        *sample = i as f32;
    }
    buffer
}

#[test]
fn a_buffer_the_audio_touched_is_wired_after_a_scan() {
    let buffer = touched_buffer();
    wire_private_memory();
    assert!(
        user_wired_count(buffer.as_ptr() as *const u8) > 0,
        "an 8 MB buffer the audio side touched must be wired, or the kernel \
         may compress it and the dsp-worker stalls on it"
    );
}

#[test]
fn a_later_scan_does_not_wire_the_same_region_again() {
    let buffer = touched_buffer();
    wire_private_memory();
    let once = user_wired_count(buffer.as_ptr() as *const u8);
    wire_private_memory();
    wire_private_memory();
    assert_eq!(
        (once, user_wired_count(buffer.as_ptr() as *const u8)),
        (1, 1),
        "a region is wired once; every rescan stacking a wire would pin it \
         forever and overflow the kernel's wire count"
    );
}

#[test]
fn concurrent_scans_wire_a_region_once() {
    for _ in 0..50 {
        let buffer = touched_buffer();
        let start = std::sync::Barrier::new(8);
        std::thread::scope(|scope| {
            for _ in 0..8 {
                scope.spawn(|| {
                    start.wait();
                    wire_private_memory();
                });
            }
        });
        assert_eq!(
            user_wired_count(buffer.as_ptr() as *const u8),
            1,
            "two passes at once (the keeper and a start-up wake, two engines) \
             must not both wire the same region"
        );
    }
}

#[test]
fn address_space_nobody_touched_is_never_wired() {
    let len = 64 << 20;
    let reserved = unsafe {
        mmap(
            std::ptr::null_mut(),
            len,
            PROT_READ | PROT_WRITE,
            MAP_PRIVATE | MAP_ANON,
            APP_TAG_250,
            0,
        )
    };
    assert!(
        !reserved.is_null() && reserved as isize != -1,
        "mmap failed"
    );
    wire_private_memory();
    let wired = user_wired_count(reserved as *const u8);
    unsafe { munmap(reserved, len) };
    assert_eq!(
        wired, 0,
        "a 64 MB reservation nobody touched must not be wired — wiring faults \
         the whole range into RAM"
    );
}

#[test]
fn memory_allocated_outside_the_audio_is_never_wired() {
    assert!(engine::audio_zone_router::install());
    // The UI's, the loader's, a rebuild's scratch: touched, then left alone.
    let mut other = vec![0.0_f32; 2 << 20];
    for (i, sample) in other.iter_mut().enumerate().step_by(1024) {
        *sample = i as f32;
    }
    wire_private_memory();
    assert_eq!(
        user_wired_count(other.as_ptr() as *const u8),
        0,
        "only the audio's memory is wired; wiring the rest keeps every byte \
         the app ever freed resident"
    );
}
