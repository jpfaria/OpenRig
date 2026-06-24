//! #716 RED (#6/#7 device layer) — opening the audio device for a bound chain
//! must read its I/O from the SYSTEM binding, not from the chain's blocks.
//!
//! User's design: INPUT/OUTPUT are not part of the effect chain — they say
//! where sound comes from / goes to. The engine generates one stream per
//! input×output combination, taking the I/O from the per-machine E/S binding
//! (not from the chain). The chain holds only effects.
//!
//! Bug: `resolve_chain_inputs` (the cpal device-resolution path) reads
//! `chain.blocks` Input blocks — EMPTY for a binding-bound chain — and bails
//! "has no input blocks configured". So no device opens → no runtime → no sound
//! on activate, and block toggles report "no live runtime".
//!
//! This drives the pub `resolve_project_chain_sample_rates` (which runs the
//! device-resolution per chain) and asserts it no longer bails for lack of
//! chain blocks — the binding must provide the I/O. (The final device-open is
//! hardware; that is the OPENRIG_HW_TESTS battery.)

use domain::ids::{ChainId, DeviceId};
use domain::io_binding::{ChannelMode, IoBinding, IoEndpoint};
use infra_cpal::resolve_project_chain_sample_rates;
use project::chain::Chain;
use project::project::Project;

fn one_binding() -> Vec<IoBinding> {
    vec![IoBinding {
        id: "io".into(),
        name: "Interface".into(),
        inputs: vec![IoEndpoint {
            name: "in".into(),
            device_id: DeviceId("dev".into()),
            mode: ChannelMode::Mono,
            channels: vec![0],
        }],
        outputs: vec![IoEndpoint {
            name: "out".into(),
            device_id: DeviceId("dev".into()),
            mode: ChannelMode::Mono,
            channels: vec![0],
        }],
    }]
}

fn checklist_bound_chain() -> Chain {
    Chain {
        id: ChainId("rig:input-1".into()),
        description: None,
        instrument: "electric_guitar".into(),
        enabled: true,
        volume: 100.0,
        io_binding_ids: vec!["io".into()],
        blocks: vec![],
    }
}

#[test]
fn device_resolution_reads_io_from_binding_not_chain_blocks() {
    let project = Project {
        name: None,
        device_settings: vec![],
        chains: vec![checklist_bound_chain()],
        midi: None,
    };

    let result = resolve_project_chain_sample_rates(&project, &one_binding());

    // It may still error because the test device "dev" does not physically
    // exist (real device-open is hardware) — but it must NOT bail for lack of
    // chain Input blocks: the I/O comes from the binding.
    if let Err(e) = &result {
        let msg = format!("{e:#}");
        assert!(
            !msg.contains("has no input blocks") && !msg.contains("no output blocks"),
            "device resolution for a binding-bound chain must read I/O from the system binding, \
             not the chain's (empty) blocks; it bailed with: {msg}"
        );
    }
}
