//! Issue #980 — once the audio engine starts, the memory its chains touch
//! stays resident for as long as OpenRig runs.
//!
//! On the owner's rig, with the machine out of memory, the kernel compressed
//! pages the reverbs touch only every few seconds and the dsp-worker stalled
//! decompressing them: 1152..50816 frames lost per 90 s run on the hardware
//! test (`issue_980_owners_two_guitars_two_outputs`); wiring the process's
//! memory took it to 0 in 6/6 runs. Here, without a device: after the engine
//! started, a buffer allocated later — a chain rebuilt, a plugin enabled —
//! must end up wired. macOS only (the kernel compressor is what this answers).
#![cfg(target_os = "macos")]

use std::time::{Duration, Instant};

use infra_cpal::ProjectRuntimeController;
use project::project::Project;

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
}

/// `user_wired_count` of the region holding `ptr` (`vm_region_basic_info_64`,
/// low half of word 8).
fn user_wired_count(ptr: *const u8) -> u16 {
    let (mut address, mut size, mut object) = (ptr as u64, 0u64, 0u32);
    let mut info = [0i32; 9];
    let mut count = 9u32;
    let kr = unsafe {
        mach_vm_region(
            mach_task_self_,
            &mut address,
            &mut size,
            9,
            info.as_mut_ptr(),
            &mut count,
            &mut object,
        )
    };
    assert_eq!(kr, 0, "mach_vm_region failed");
    (info[8] as u32 & 0xFFFF) as u16
}

#[test]
fn memory_allocated_after_the_engine_started_ends_up_wired() {
    let project = Project {
        name: None,
        device_settings: Vec::new(),
        chains: Vec::new(),
        midi: None,
    };
    let _engine = ProjectRuntimeController::start(&project).expect("start the engine");
    std::thread::sleep(Duration::from_secs(1));

    // A reverb's delay line, allocated and written through once.
    let mut delay_line = vec![0.0_f32; 2 << 20];
    for (i, sample) in delay_line.iter_mut().enumerate().step_by(1024) {
        *sample = i as f32;
    }
    let deadline = Instant::now() + Duration::from_secs(15);
    while user_wired_count(delay_line.as_ptr() as *const u8) == 0 {
        assert!(
            Instant::now() < deadline,
            "memory the chains touch must be wired while the engine runs, or \
             the kernel compresses it and the dsp-worker stalls on it"
        );
        std::thread::sleep(Duration::from_millis(100));
    }
}
