//! The worker hands the chain the capture time of each buffer it processes, so
//! a stamped input tap knows when its first sample entered the interface.

use std::sync::atomic::Ordering;
use std::sync::Arc;
use std::time::{Duration, Instant};

use domain::ids::{ChainId, DeviceId};
use domain::io_binding::{ChannelMode, IoBinding, IoEndpoint};
use engine::runtime::build_chain_runtime_state;
use project::chain::Chain;

use crate::LiveRuntimeSlot;

fn registry() -> Vec<IoBinding> {
    vec![IoBinding {
        id: "io".into(),
        name: "IO".into(),
        inputs: vec![IoEndpoint {
            name: "in0".into(),
            device_id: DeviceId("dev".into()),
            mode: ChannelMode::Mono,
            channels: vec![0],
        }],
        outputs: vec![IoEndpoint {
            name: "out0".into(),
            device_id: DeviceId("dev".into()),
            mode: ChannelMode::Stereo,
            channels: vec![0, 1],
        }],
    }]
}

fn chain() -> Chain {
    Chain {
        id: ChainId("capture-stamp".into()),
        description: None,
        instrument: "electric_guitar".into(),
        enabled: true,
        volume: 100.0,
        io_binding_ids: vec!["io".into()],
        blocks: vec![],
        di_output: None,
        loopers: vec![],
        disabled_endpoints: Default::default(),
        mix: Default::default(),
    }
}

#[test]
fn a_buffer_pushed_with_its_capture_time_stamps_the_input_tap() {
    let runtime = Arc::new(
        build_chain_runtime_state(&chain(), 48_000.0, &[256], &registry())
            .expect("the runtime must build"),
    );
    let slot = LiveRuntimeSlot::new(Arc::clone(&runtime));
    let (_rings, first_capture_ns) = runtime.subscribe_input_tap_stamped(0, 1, &[0], 4_096);

    let producer = super::spawn("capture".into(), slot.handle(), 0, 1, 48_000, 64 * 8, None);
    producer.push(&[0.25; 64], 777);

    let deadline = Instant::now() + Duration::from_secs(2);
    while first_capture_ns.load(Ordering::Relaxed) == 0 && Instant::now() < deadline {
        std::thread::sleep(Duration::from_millis(1));
    }
    assert_eq!(first_capture_ns.load(Ordering::Relaxed), 777);
}
