//! #716 RED — the chain editor's binding CHECKLIST must actually route audio.
//!
//! The GUI checklist persists the user's selection as `Chain.io_binding_ids`
//! (a `Vec<String>` of binding ids). But the LIVE build seam
//! (`build_chain_runtime`) routes binding-only by PER-BLOCK `io`/`endpoint`
//! (see `issue_716_live_path_binding_routing.rs`, where bound chains carry
//! `bound_input(.. io ..)` blocks and `io_binding_ids` is EMPTY).
//!
//! So a chain configured purely via the checklist — `io_binding_ids = ["io_a"]`
//! with no `io`-bearing Input/Output blocks — produces NO runtime on the live
//! path. That single gap is the shared root cause the user is hitting:
//!   * no sound from a checklist-bound chain,
//!   * toggling/adding a block does nothing,
//!   * "block '…' not found in any input runtime of the chain".
//!
//! This test pins the user-facing expectation: selecting a binding in the
//! checklist MUST yield a working per-input runtime. It is RED until the
//! `io_binding_ids` selection is honoured by the build path.

use domain::ids::{ChainId, DeviceId};
use domain::io_binding::{ChannelMode, IoBinding, IoEndpoint};
use infra_cpal::{build_chain_runtime, BuildRequest};
use project::chain::Chain;

/// One single-device mono binding (input + output on dev_a).
fn binding_io_a() -> IoBinding {
    IoBinding {
        id: "io_a".into(),
        name: "Device A".into(),
        inputs: vec![IoEndpoint {
            name: "in_a".into(),
            device_id: DeviceId("dev_a".into()),
            mode: ChannelMode::Mono,
            channels: vec![0],
        }],
        outputs: vec![IoEndpoint {
            name: "out_a".into(),
            device_id: DeviceId("dev_a".into()),
            mode: ChannelMode::Mono,
            channels: vec![0],
        }],
    }
}

/// The chain exactly as the editor checklist saves it: a binding is SELECTED
/// (`io_binding_ids`), the chain carries only effect blocks (here none), and
/// there are NO per-block `io`/`endpoint` Input/Output blocks.
fn checklist_bound_chain() -> Chain {
    Chain {
        id: ChainId("rig:input-1".into()),
        description: Some("checklist-bound chain".into()),
        instrument: "electric_guitar".into(),
        enabled: true,
        volume: 100.0,
        io_binding_ids: vec!["io_a".into()],
        blocks: vec![],
    }
}

#[test]
fn checklist_selected_binding_builds_a_runtime() {
    let req = BuildRequest {
        chain: checklist_bound_chain(),
        sample_rate: 48_000.0,
        buffer_sizes: vec![1024],
        io_bindings: vec![binding_io_a()],
    };

    let runtimes =
        build_chain_runtime(&req).expect("checklist-bound chain must build cleanly");

    assert_eq!(
        runtimes.len(),
        1,
        "selecting binding 'io_a' in the checklist (io_binding_ids) must yield exactly one \
         isolated input runtime; got {} — the live build path ignores io_binding_ids and only \
         routes per-block io/endpoint, so the checklist-bound chain produces NO runtime \
         (no sound, toggles do nothing, 'block not found in any input runtime')",
        runtimes.len()
    );
}
