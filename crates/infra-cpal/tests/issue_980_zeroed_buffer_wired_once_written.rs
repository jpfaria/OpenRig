//! Issue #980 (review of #981) — a buffer a new runtime allocates zeroed and
//! writes only once it plays must be wired soon after that first write, not
//! at the keeper's next 5 s pass.
//!
//! A delay line is `vec![0.0; n]`: calloc hands it fresh pages the kernel has
//! not even backed yet (no VM object: `SM_EMPTY`, 0 resident pages), so the
//! pass the runtime's going-live wakes skips it as untouched. The DSP writes
//! it a moment later — and until the next periodic pass the kernel may
//! compress it, the window the hardware trace showed losing audio. macOS only.
#![cfg(target_os = "macos")]

use std::sync::Arc;
use std::time::{Duration, Instant};

use domain::ids::{ChainId, DeviceId};
use domain::io_binding::{ChannelMode, IoBinding, IoEndpoint};
use engine::runtime::{build_chain_runtime_state, ChainRuntimeState};
use infra_cpal::{LiveRuntimeSlot, ProjectRuntimeController};
use project::chain::Chain;
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

fn runtime() -> Arc<ChainRuntimeState> {
    let chain = Chain {
        id: ChainId("t".into()),
        description: None,
        instrument: "electric_guitar".into(),
        enabled: true,
        volume: 100.0,
        io_binding_ids: vec!["io".into()],
        blocks: vec![],
        di_output: None,
        loopers: vec![],
        mix: Default::default(),
    };
    let endpoint = |name: &str, mode, channels: Vec<usize>| IoEndpoint {
        name: name.into(),
        device_id: DeviceId("d".into()),
        mode,
        channels,
    };
    let registry = vec![IoBinding {
        id: "io".into(),
        name: "IO".into(),
        inputs: vec![endpoint("in", ChannelMode::Mono, vec![0])],
        outputs: vec![endpoint("out", ChannelMode::Stereo, vec![0, 1])],
    }];
    Arc::new(build_chain_runtime_state(&chain, 48_000.0, &[256], &registry).unwrap())
}

#[test]
fn a_zeroed_buffer_is_wired_soon_after_its_first_write() {
    let project = Project {
        name: None,
        device_settings: Vec::new(),
        chains: Vec::new(),
        midi: None,
    };
    let _engine = ProjectRuntimeController::start(&project).expect("start the engine");
    std::thread::sleep(Duration::from_millis(1_500));

    // A 16 MB delay line allocated zeroed: fresh pages, nothing written yet.
    let mut delay_line = vec![0.0_f32; 4 << 20];
    let _slot = LiveRuntimeSlot::new(runtime());
    // The runtime plays: its DSP writes the line after the wake-up pass.
    std::thread::sleep(Duration::from_millis(200));
    delay_line[0] = 1.0;
    std::hint::black_box(&delay_line);

    let deadline = Instant::now() + Duration::from_millis(2_500);
    while user_wired_count(delay_line.as_ptr() as *const u8) == 0 {
        assert!(
            Instant::now() < deadline,
            "a zeroed buffer written right after its runtime went live must be \
             wired within 2.5 s, not at the keeper's next 5 s pass"
        );
        std::thread::sleep(Duration::from_millis(20));
    }
}
