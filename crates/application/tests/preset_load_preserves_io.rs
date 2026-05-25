//! Bug repro (user, screenshot 1): switching the active preset on a
//! chain (CABELINHO `rig:input-4`, dropdown / position pick) drops the
//! chain to "INVALID PROJECT: chain 'rig:input-4' has no output blocks".
//!
//! Hypothesis: the preset-switch path the dropdown triggers is
//! `Command::LoadChainPreset` (preset_blocks intentionally I/O-stripped),
//! and the handler replaces `chain.blocks` outright without preserving
//! the existing I/O. The dispatcher fix in `local_dispatcher_rig.rs`
//! only covers `ApplyRigNav`, so LoadChainPreset still wipes outputs.
//!
//! This RED test calls `Command::LoadChainPreset` with I/O-less preset
//! blocks and asserts the chain still has its user-configured output
//! after the dispatch.

use std::cell::RefCell;
use std::rc::Rc;

use domain::ids::{BlockId, ChainId, DeviceId};
use project::block::{
    AudioBlock, AudioBlockKind, CoreBlock, InputBlock, InputEntry, OutputBlock, OutputEntry,
};
use project::chain::{Chain, ChainInputMode, ChainOutputMode};
use project::param::ParameterSet;
use project::project::Project;

use application::command::Command;
use application::dispatcher::CommandDispatcher;
use application::local_dispatcher::LocalDispatcher;

const CHAIN_ID: &str = "rig:input-4";
const DEVICE: &str = "test:device";

fn user_input() -> AudioBlock {
    AudioBlock {
        id: BlockId("rig:input-4:in".into()),
        enabled: true,
        kind: AudioBlockKind::Input(InputBlock {
            model: "standard".into(),
            entries: vec![InputEntry {
                device_id: DeviceId(DEVICE.into()),
                mode: ChainInputMode::Mono,
                channels: vec![0],
            }],
        }),
    }
}

fn user_output() -> AudioBlock {
    AudioBlock {
        id: BlockId("rig:input-4:out".into()),
        enabled: true,
        kind: AudioBlockKind::Output(OutputBlock {
            model: "standard".into(),
            entries: vec![OutputEntry {
                device_id: DeviceId(DEVICE.into()),
                mode: ChainOutputMode::Stereo,
                channels: vec![0, 1],
            }],
        }),
    }
}

fn core(id: &str) -> AudioBlock {
    AudioBlock {
        id: BlockId(id.into()),
        enabled: true,
        kind: AudioBlockKind::Core(CoreBlock {
            effect_type: "gain".into(),
            model: "volume".into(),
            params: ParameterSet::default(),
        }),
    }
}

fn dispatcher_with_chain_having_io() -> (LocalDispatcher, Rc<RefCell<Project>>) {
    let chain = Chain {
        id: ChainId(CHAIN_ID.into()),
        description: Some("CABELINHO".into()),
        instrument: "electric_guitar".into(),
        enabled: true,
        volume: 100.0,
        blocks: vec![user_input(), core("eq:1"), core("amp:2"), user_output()],
    };
    let project = Rc::new(RefCell::new(Project {
        name: Some("test".into()),
        device_settings: Vec::new(),
        chains: vec![chain],
        midi: None,
    }));
    let dispatcher = LocalDispatcher::new(Rc::clone(&project));
    (dispatcher, project)
}

fn outputs_count(project: &Rc<RefCell<Project>>) -> usize {
    project
        .borrow()
        .chains
        .iter()
        .find(|c| c.id.0 == CHAIN_ID)
        .map(|c| {
            c.blocks
                .iter()
                .filter(|b| matches!(b.kind, AudioBlockKind::Output(_)))
                .count()
        })
        .unwrap_or(0)
}

fn inputs_count(project: &Rc<RefCell<Project>>) -> usize {
    project
        .borrow()
        .chains
        .iter()
        .find(|c| c.id.0 == CHAIN_ID)
        .map(|c| {
            c.blocks
                .iter()
                .filter(|b| matches!(b.kind, AudioBlockKind::Input(_)))
                .count()
        })
        .unwrap_or(0)
}

#[test]
fn load_chain_preset_with_io_less_payload_preserves_user_output() {
    let (d, p) = dispatcher_with_chain_having_io();
    assert_eq!(outputs_count(&p), 1, "precondition: 1 output");
    assert_eq!(inputs_count(&p), 1, "precondition: 1 input");

    // The adapter docs say preset_blocks is "I/O-stripped" by design --
    // the file contains only the effect chain. The dispatcher must merge
    // those effect blocks with the chain's existing I/O endpoints.
    let preset_blocks = vec![core("preset:1"), core("preset:2")];
    let _ = d.dispatch(Command::LoadChainPreset {
        chain: ChainId(CHAIN_ID.into()),
        preset_blocks,
    });

    assert_eq!(
        outputs_count(&p),
        1,
        "LoadChainPreset must keep the user's output endpoint (preset \
         files intentionally do not carry I/O)"
    );
    assert_eq!(
        inputs_count(&p),
        1,
        "LoadChainPreset must keep the user's input endpoint"
    );
}
