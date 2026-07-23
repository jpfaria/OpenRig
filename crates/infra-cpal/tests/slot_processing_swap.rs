//! Issue #672 — the stream-callback processing seam reads the chain's *live*
//! runtime through `LiveRuntimeSlot`, so a worker-published runtime takes effect
//! on the next buffer. Proven hardware-free by driving the exact realtime engine
//! functions (`process_input_f32` / `process_output_f32_mixed`) the CPAL callback
//! runs, through the seam helpers.
//!
//! Distinguisher: publish a *draining* runtime. The live seam then drops input
//! and `process_output_f32` fills silence; a seam that kept a stale captured
//! `Arc` (the bug we fix) would keep producing the active runtime's signal.

use std::sync::Arc;

use domain::ids::{ChainId, DeviceId};
use domain::io_binding::{ChannelMode, IoBinding, IoEndpoint};
use engine::runtime::build_chain_runtime_state;
use infra_cpal::{process_input_buffer, process_output_buffer, LiveRuntimeSlot};
use project::chain::Chain;

const SR: f32 = 48_000.0;
const FRAMES: usize = 256;

/// Model A (#716): a mono-in/stereo-out passthrough whose endpoints live in
/// the "io" binding (`io_registry`), not block `entries`.
fn passthrough_chain(id: &str) -> Chain {
    Chain {
        id: ChainId(id.into()),
        description: None,
        instrument: "electric_guitar".into(),
        enabled: true,
        volume: 100.0,
        io_binding_ids: vec!["io".into()],
        blocks: vec![],
        di_output: None,
    }
}

fn io_registry() -> Vec<IoBinding> {
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

/// Drive one buffer through the seam: feed `sig` via the live input runtime,
/// read the mixed output via the live output runtime.
fn drive(
    slot: &LiveRuntimeSlot,
    loaded: &mut Vec<Arc<engine::runtime::ChainRuntimeState>>,
    sig: &[f32],
) -> Vec<f32> {
    process_input_buffer(slot, 0, sig, 1);
    let mut out = vec![0.0_f32; sig.len() * 2];
    let mut scratch = vec![0.0_f32; sig.len() * 2];
    process_output_buffer(
        std::slice::from_ref(slot),
        loaded,
        0,
        &mut out,
        2,
        &mut scratch,
    );
    out
}

#[test]
fn seam_follows_published_runtime() {
    let chain = passthrough_chain("seam");
    let registry = io_registry();
    let rt_a = Arc::new(build_chain_runtime_state(&chain, SR, &[FRAMES], &registry).unwrap());
    let slot = LiveRuntimeSlot::new(Arc::clone(&rt_a));
    let mut loaded = Vec::with_capacity(2);

    let sig: Vec<f32> = (0..FRAMES)
        .map(|i| 0.4 * (2.0 * std::f32::consts::PI * 220.0 * i as f32 / SR).sin())
        .collect();

    // Warm up runtime A and capture the steady output — it must carry signal.
    let mut out_a = Vec::new();
    for _ in 0..16 {
        out_a = drive(&slot, &mut loaded, &sig);
    }
    assert!(
        out_a.iter().any(|s| s.abs() > 1e-4),
        "active runtime A must produce non-silent passthrough output"
    );

    // Publish a draining runtime B. The live seam must now read B: input is
    // dropped and process_output_f32 fills silence.
    let rt_b = Arc::new(build_chain_runtime_state(&chain, SR, &[FRAMES], &registry).unwrap());
    assert!(!Arc::ptr_eq(&rt_a, &rt_b));
    rt_b.set_draining();
    let _old = slot.publish(rt_b);

    let mut out_b = Vec::new();
    for _ in 0..16 {
        out_b = drive(&slot, &mut loaded, &sig);
    }
    assert!(
        out_b.iter().all(|s| s.abs() <= 1e-4),
        "after publishing a draining runtime, the live seam drains to silence — \
         proving the callback reads the slot, not a stale captured Arc"
    );
}
