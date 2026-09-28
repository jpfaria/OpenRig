//! Issue #980 — a chain's memory is wired the moment its runtime goes live,
//! not at the memory keeper's next periodic pass.
//!
//! Hardware trace on the owner's rig with the keeper passing every 5 s: every
//! late dsp-worker buffer of the run fell between the chain coming up
//! (20:16:48.6) and the pass that wired its 655 MB (20:16:53.8) — the kernel
//! compressed the new chain's pages within those seconds and the idle phase
//! lost 128-512 frames. A runtime installed in a live slot, at start-up
//! (`LiveRuntimeSlot::new`) or by a live rebuild (`publish`), must get its
//! memory wired right away. macOS only.
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

/// A reverb's delay line: allocated, then written through once.
fn delay_line() -> Vec<f32> {
    let mut buffer = vec![0.0_f32; 2 << 20];
    for (i, sample) in buffer.iter_mut().enumerate().step_by(1024) {
        *sample = i as f32;
    }
    buffer
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

/// Whether the region holding `buffer` is wired within `within`.
fn wired_within(buffer: &[f32], within: Duration) -> bool {
    let deadline = Instant::now() + within;
    while Instant::now() < deadline {
        if user_wired_count(buffer.as_ptr() as *const u8) > 0 {
            return true;
        }
        std::thread::sleep(Duration::from_millis(20));
    }
    false
}

#[test]
fn a_runtime_going_live_has_its_memory_wired_at_once() {
    let project = Project {
        name: None,
        device_settings: Vec::new(),
        chains: Vec::new(),
        midi: None,
    };
    let _engine = ProjectRuntimeController::start(&project).expect("start the engine");
    // Past the keeper's first pass: only a wake-up can wire what comes next.
    std::thread::sleep(Duration::from_millis(1_500));

    let at_start = delay_line();
    let slot = LiveRuntimeSlot::new(runtime());
    let started = wired_within(&at_start, Duration::from_secs(1));

    let on_rebuild = delay_line();
    let _superseded = slot.publish(runtime());
    let rebuilt = wired_within(&on_rebuild, Duration::from_secs(1));

    assert_eq!(
        (started, rebuilt),
        (true, true),
        "memory of a runtime going live (start-up, live rebuild) must be wired \
         within 1 s — the kernel compressed a new chain's pages within the 5 s \
         the periodic pass took"
    );
}
