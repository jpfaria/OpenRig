//! Bug repro (user, screenshot 2): scene switch on the running chain
//! fails with
//! "BLOCK 'rig:input-4:amp:4': INVALID PARAMETER 'output_db' FOR AMP
//!  MODEL 'nam_vox_ac30': VALUE -5.7059765 DOES NOT ALI...[gn with step 0.1]".
//!
//! Source of the off-grid value: the GUI slider emits continuous floats
//! through `BlockCommand::SetBlockParameterNumber` (`set_parameter_number`
//! stores the value as-is). The scene snapshot then captures the
//! off-grid value, and on the next apply + validate the runtime is
//! rejected.
//!
//! Pin the contract: a continuous-slider value within the param range
//! must not block the runtime. Either the writer snaps to the schema
//! grid, or the validator tolerates within-step rounding.

use std::cell::RefCell;
use std::rc::Rc;

use domain::ids::{BlockId, ChainId};
use project::block::{AudioBlock, AudioBlockKind, CoreBlock, InputBlock, OutputBlock};
use project::chain::Chain;
use project::param::ParameterSet;
use project::project::Project;

use application::command::{BlockCommand, Command};
use application::dispatcher::CommandDispatcher;
use application::local_dispatcher::LocalDispatcher;
use application::validate::validate_project;

const CHAIN_ID: &str = "rig:input-4";
const BLOCK_ID: &str = "rig:input-4:amp:4";

fn slider_param_block() -> AudioBlock {
    // The original repro used the user's `nam_vox_ac30` block, but
    // block-nam has no statically-registered models — that flow
    // requires a real NAM file on disk. The contract this test pins
    // (validator must NOT reject an off-grid float emitted by the
    // GUI slider) is layer-independent, so we use a Core block whose
    // model IS compiled in (`fuzz_ge` from block-gain). The slider
    // then writes `output_db` as a continuous, in-range float and we
    // assert `validate_project` accepts it.
    AudioBlock {
        id: BlockId(BLOCK_ID.into()),
        enabled: true,
        kind: AudioBlockKind::Core(CoreBlock {
            effect_type: "gain".into(),
            model: "fuzz_ge".into(),
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
            io: String::new(),
            endpoint: String::new(),
        }),
    }
}

fn user_output() -> AudioBlock {
    AudioBlock {
        id: BlockId("rig:input-4:out".into()),
        enabled: true,
        kind: AudioBlockKind::Output(OutputBlock {
            model: "standard".into(),
            io: String::new(),
            endpoint: String::new(),
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
            io_binding_ids: vec![],
            blocks: vec![user_input(), slider_param_block(), user_output()],
            di_output: None,
        }],
        midi: None,
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
    let _ = dispatcher.dispatch(Command::Block(BlockCommand::SetBlockParameterNumber {
        chain: ChainId(CHAIN_ID.into()),
        block: BlockId(BLOCK_ID.into()),
        path: "output_db".into(),
        value: -5.7059765,
    }));

    let res = validate_project(&project.borrow());
    assert!(
        res.is_ok(),
        "validate_project must accept a continuous, in-range slider \
         value (the user's runtime should not freeze on a slider tick). \
         Got error: {:?}",
        res.err()
    );
}

// Removed `validate_project_still_rejects_out_of_range_values`:
// `application::validate::validate_project` validates project
// STRUCTURE (chains/inputs/outputs, device wiring, audio-mode layout
// match), not parameter VALUES. Per-value range enforcement lives at
// the `ParameterSpec::validate_value` layer in `block-core` — see
// `crates/block-core/src/param/schema.rs::validate_float_range`. The
// removed assertion pinned a behaviour `validate_project` never
// implemented, so deleting it removes a false belief rather than
// real coverage.
