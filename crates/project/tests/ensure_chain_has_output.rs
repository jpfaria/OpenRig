//! Live emergency repro: a rig-backed chain whose `rig.outputs` is empty
//! lands in the user's project (today: every CABELINHO-style chain that
//! was saved before the `SaveChainOutputEndpoints` rig-persistence fix
//! merged). On open the chain has NO Output block, `validate_project`
//! rejects it ("invalid project: chain 'rig:input-4' has no output
//! blocks"), the runtime won't start, no audio.
//!
//! Migration fix: on load, ensure every chain has at least one Output
//! block; when missing, synthesize a sensible default that the user can
//! later customise via the I/O editor. The user always gets sound on
//! open; broken-state projects self-heal forward.

use domain::ids::{BlockId, ChainId, DeviceId};
use project::block::{AudioBlock, AudioBlockKind, InputBlock, InputEntry, OutputBlock};
use project::chain::{Chain, ChainInputMode};
use project::project::Project;
use project::project_ensure_io::ensure_chains_have_output;

const DEVICE: &str = "coreaudio:default";

fn chain_with_only_input(id: &str) -> Chain {
    Chain {
        id: ChainId(id.into()),
        description: Some("test".into()),
        instrument: "electric_guitar".into(),
        enabled: false,
        volume: 100.0,
        blocks: vec![AudioBlock {
            id: BlockId(format!("{id}:in")),
            enabled: true,
            kind: AudioBlockKind::Input(InputBlock {
                model: "standard".into(),
                io: String::new(),
                endpoint: String::new(),
                entries: vec![InputEntry {
                    device_id: DeviceId(DEVICE.into()),
                    mode: ChainInputMode::Mono,
                    channels: vec![0],
                }],
            }),
        }],
    }
}

fn outputs_count(chain: &Chain) -> usize {
    chain
        .blocks
        .iter()
        .filter(|b| matches!(b.kind, AudioBlockKind::Output(_)))
        .count()
}

#[test]
fn ensure_chains_have_output_appends_default_when_missing() {
    let mut project = Project {
        name: Some("test".into()),
        device_settings: Vec::new(),
        chains: vec![chain_with_only_input("rig:input-4")],
        midi: None,
    };
    ensure_chains_have_output(&mut project, &DeviceId(DEVICE.into()));
    let chain = &project.chains[0];
    assert_eq!(
        outputs_count(chain),
        1,
        "load-time helper must synthesize a default Output so old projects \
         with empty rig.outputs are not silenced"
    );
    let Some(AudioBlockKind::Output(ob)) = chain.blocks.iter().last().map(|b| &b.kind) else {
        panic!("synthesized block must be Output and at chain tail");
    };
    assert_eq!(ob.entries.len(), 1, "default Output gets one entry");
    assert_eq!(ob.entries[0].device_id.0, DEVICE);
}

#[test]
fn ensure_chains_have_output_is_no_op_when_output_present() {
    let mut chain = chain_with_only_input("rig:input-4");
    chain.blocks.push(AudioBlock {
        id: BlockId("rig:input-4:out".into()),
        enabled: true,
        kind: AudioBlockKind::Output(OutputBlock {
            model: "standard".into(),
            io: String::new(),
            endpoint: String::new(),
            entries: Vec::new(),
        }),
    });
    let original = chain.blocks.clone();
    let mut project = Project {
        name: Some("test".into()),
        device_settings: Vec::new(),
        chains: vec![chain],
        midi: None,
    };
    ensure_chains_have_output(&mut project, &DeviceId(DEVICE.into()));
    assert_eq!(
        project.chains[0].blocks, original,
        "chain that already has an Output must be left unchanged"
    );
}

#[test]
fn ensure_chains_have_output_does_nothing_when_default_device_empty() {
    // No fallback device available → leave chain alone (better to surface
    // the validate error than silently insert a useless empty Output).
    let mut project = Project {
        name: Some("test".into()),
        device_settings: Vec::new(),
        chains: vec![chain_with_only_input("rig:input-4")],
        midi: None,
    };
    ensure_chains_have_output(&mut project, &DeviceId(String::new()));
    assert_eq!(
        outputs_count(&project.chains[0]),
        0,
        "with no usable default device, leave the chain unchanged so the \
         user sees the validate error and configures one"
    );
}
