//! Issue #980 (review of #981) — the console and headless-rig frontends
//! build their streams with `build_streams_for_project` and never start a
//! `ProjectRuntimeController`, so on a Mac out of memory their chains had the
//! full exposure the memory keeper removes from the app: reverb delay lines
//! compressed, the dsp-worker stalling on them, the output underrunning.
//! Building a project's streams must keep the audio's memory resident too.
//! macOS only; no device needed (an empty project opens no stream).
#![cfg(target_os = "macos")]

use std::time::{Duration, Instant};

use engine::runtime::RuntimeGraph;
use infra_cpal::build_streams_for_project;
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
fn building_a_projects_streams_keeps_the_audio_memory_resident() {
    let project = Project {
        name: None,
        device_settings: Vec::new(),
        chains: Vec::new(),
        midi: None,
    };
    let graph = RuntimeGraph {
        chains: std::collections::HashMap::new(),
    };
    let streams = build_streams_for_project(&project, &graph, &[]).expect("build streams");
    assert!(streams.is_empty());
    std::thread::sleep(Duration::from_millis(1_500));

    // A reverb's delay line, allocated and written through once.
    let mut delay_line = vec![0.0_f32; 2 << 20];
    for (i, sample) in delay_line.iter_mut().enumerate().step_by(1024) {
        *sample = i as f32;
    }
    let deadline = Instant::now() + Duration::from_secs(15);
    while user_wired_count(delay_line.as_ptr() as *const u8) == 0 {
        assert!(
            Instant::now() < deadline,
            "the console / headless-rig path must keep the audio's memory \
             resident like the app does"
        );
        std::thread::sleep(Duration::from_millis(100));
    }
}
