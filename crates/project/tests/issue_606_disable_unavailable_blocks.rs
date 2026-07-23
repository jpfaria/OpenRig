//! Issue #606 — on load, a block whose model is unavailable (an uninstalled
//! NAM/IR/LV2 pack, or a native unsupported on this platform) must be
//! disabled (`enabled = false`) so the chain keeps playing with the block
//! bypassed and the user is not left staring at a silently-faulted-but-"on"
//! pedal. Available blocks (native models or cataloged disk packages) are
//! left untouched.

use domain::ids::{BlockId, ChainId};
use project::block::{AudioBlock, AudioBlockKind, CoreBlock, InputBlock};
use project::chain::Chain;
use project::param::ParameterSet;
use project::project::Project;
use project::project_disable_unavailable::disable_unavailable_blocks;

fn gain_block(id: &str, model: &str) -> AudioBlock {
    AudioBlock {
        id: BlockId(id.into()),
        enabled: true,
        kind: AudioBlockKind::Core(CoreBlock {
            effect_type: "gain".into(),
            model: model.into(),
            params: ParameterSet::default(),
        }),
    }
}

fn input_block(id: &str) -> AudioBlock {
    AudioBlock {
        id: BlockId(id.into()),
        enabled: true,
        kind: AudioBlockKind::Input(InputBlock {
            model: "standard".into(),
            io: String::new(),
            endpoint: String::new(),
        }),
    }
}

fn chain(blocks: Vec<AudioBlock>) -> Chain {
    Chain {
        id: ChainId("c".into()),
        description: Some("issue-606".into()),
        instrument: "electric_guitar".into(),
        enabled: true,
        volume: 100.0,
        io_binding_ids: vec![],
        blocks,
        di_output: None,
        loopers: vec![],
    }
}

#[test]
fn disables_block_with_uninstalled_model_and_keeps_available_ones() {
    // `ibanez_ts9` is a native gain model (always available). The `nam_` id
    // is not in the catalog (no pack on disk) → unavailable. The Input block
    // is routing-only and must never be touched.
    let mut project = Project {
        name: None,
        device_settings: Vec::new(),
        chains: vec![chain(vec![
            input_block("in"),
            gain_block("ts9", "ibanez_ts9"),
            gain_block("missing", "nam_uninstalled_pedal_for_issue_606"),
        ])],
        midi: None,
    };

    let disabled = disable_unavailable_blocks(&mut project);

    let blocks = &project.chains[0].blocks;
    assert!(blocks[0].enabled, "routing Input block must stay enabled");
    assert!(
        blocks[1].enabled,
        "available native gain (ibanez_ts9) must stay enabled"
    );
    assert!(
        !blocks[2].enabled,
        "BUG #606: a block whose model is uninstalled must be disabled on load \
         so the chain keeps playing instead of silently faulting an 'on' pedal"
    );
    assert_eq!(
        disabled,
        vec![BlockId("missing".into())],
        "the call reports exactly the block ids it disabled"
    );
}

#[test]
fn leaves_already_disabled_unavailable_block_disabled_without_reporting_it() {
    // An unavailable block that is already off is a no-op: it stays off and
    // is NOT reported (nothing changed).
    let mut missing = gain_block("missing", "nam_uninstalled_pedal_for_issue_606");
    missing.enabled = false;
    let mut project = Project {
        name: None,
        device_settings: Vec::new(),
        chains: vec![chain(vec![input_block("in"), missing])],
        midi: None,
    };

    let disabled = disable_unavailable_blocks(&mut project);

    assert!(!project.chains[0].blocks[1].enabled);
    assert!(
        disabled.is_empty(),
        "an already-disabled block is not re-reported"
    );
}
