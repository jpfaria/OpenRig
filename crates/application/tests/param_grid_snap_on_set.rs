//! Bug repro (user, screenshot 2): scene switch on the running chain
//! fails with
//! "BLOCK 'rig:input-4:amp:4': INVALID PARAMETER 'output_db' FOR AMP
//!  MODEL 'nam_vox_ac30': VALUE -5.7059765 DOES NOT ALI...[gn with step 0.1]".
//!
//! Source of the off-grid value: the GUI slider emits continuous floats
//! through `Command::SetBlockParameterNumber` (`set_parameter_number`
//! stores the value as-is). The scene snapshot then captures the
//! off-grid value, and on the next apply + validate the runtime is
//! rejected.
//!
//! Pin the contract: a continuous-slider value within the param range
//! must not block the runtime. Either the writer snaps to the schema
//! grid, or the validator tolerates within-step rounding.

use std::cell::RefCell;
use std::rc::Rc;

use domain::ids::{BlockId, ChainId, DeviceId};
use project::block::{
    AudioBlock, AudioBlockKind, InputBlock, InputEntry, NamBlock, OutputBlock, OutputEntry,
};
use project::chain::{Chain, ChainInputMode, ChainOutputMode};
use project::param::ParameterSet;
use project::project::Project;

use application::command::Command;
use application::dispatcher::CommandDispatcher;
use application::local_dispatcher::LocalDispatcher;
use application::validate::validate_project;

const CHAIN_ID: &str = "rig:input-4";
const BLOCK_ID: &str = "rig:input-4:amp:4";
const DEVICE: &str = "test:device";

fn nam_vox_ac30_block() -> AudioBlock {
    AudioBlock {
        id: BlockId(BLOCK_ID.into()),
        enabled: true,
        kind: AudioBlockKind::Nam(NamBlock {
            model: "nam_vox_ac30".into(),
            params: ParameterSet::default(),
        }),
    }
}

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

fn project_with_amp_chain() -> Rc<RefCell<Project>> {
    Rc::new(RefCell::new(Project {
        name: Some("test".into()),
        device_settings: Vec::new(),
        chains: vec![Chain {
            id: ChainId(CHAIN_ID.into()),
            description: Some("CABELINHO".into()),
            instrument: "electric_guitar".into(),
            enabled: true,
            volume: 100.0,
            blocks: vec![user_input(), nam_vox_ac30_block(), user_output()],
        }],
    }))
}

#[test]
fn validate_project_accepts_off_grid_continuous_slider_value() {
    // Reproduces the exact user error: nam_vox_ac30 `output_db` schema
    // is step 0.1, but the GUI slider emits a continuous value like
    // -5.7059765 (not aligned to 0.1). Today validate_project rejects
    // the whole chain → audio runtime fails to start → output meter
    // freezes. The runtime should accept the in-range value (snap or
    // tolerate); pin that contract.
    let project = project_with_amp_chain();
    let dispatcher = LocalDispatcher::new(Rc::clone(&project));

    // Live action: continuous slider sets output_db.
    let _ = dispatcher.dispatch(Command::SetBlockParameterNumber {
        chain: ChainId(CHAIN_ID.into()),
        block: BlockId(BLOCK_ID.into()),
        path: "output_db".into(),
        value: -5.7059765,
    });

    let res = validate_project(&project.borrow());
    assert!(
        res.is_ok(),
        "validate_project must accept a continuous, in-range slider \
         value (the user's runtime should not freeze on a slider tick). \
         Got error: {:?}",
        res.err()
    );
}

#[test]
fn validate_project_still_rejects_out_of_range_values() {
    // Range bounds remain enforced -- only step misalignment is the
    // hotfix surface, not range overflows.
    let project = project_with_amp_chain();
    let dispatcher = LocalDispatcher::new(Rc::clone(&project));

    let _ = dispatcher.dispatch(Command::SetBlockParameterNumber {
        chain: ChainId(CHAIN_ID.into()),
        block: BlockId(BLOCK_ID.into()),
        path: "output_db".into(),
        value: 999.0, // well above the +24 dB max
    });

    let res = validate_project(&project.borrow());
    assert!(
        res.is_err(),
        "validate_project must still reject out-of-range values; only \
         step misalignment is tolerated"
    );
}
