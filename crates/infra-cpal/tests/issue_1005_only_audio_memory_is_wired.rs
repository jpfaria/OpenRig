//! While the engine runs, memory the audio did not allocate is never wired:
//! wired pages stay resident after they are freed, and wiring the whole
//! process kept the app's peak (1.2 GB with one chain) for good. macOS only,
//! like the wiring.
#![cfg(target_os = "macos")]

use std::time::Duration;

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
fn memory_outside_the_audio_stays_unwired_while_the_engine_runs() {
    let project = Project {
        name: None,
        device_settings: Vec::new(),
        chains: Vec::new(),
        midi: None,
    };
    let _engine = ProjectRuntimeController::start(&project).expect("start the engine");
    std::thread::sleep(Duration::from_secs(1));

    // A project load's scratch: allocated off the audio and touched.
    let mut scratch = vec![0.0_f32; 2 << 20];
    for (i, sample) in scratch.iter_mut().enumerate().step_by(1024) {
        *sample = i as f32;
    }
    // The start-up pass, its follow-up and a periodic pass all ran by now.
    std::thread::sleep(Duration::from_secs(6));
    assert_eq!(
        user_wired_count(scratch.as_ptr() as *const u8),
        0,
        "the keeper must wire only the audio's memory"
    );
}
