//! Bug repro (#696, user log):
//!
//! ```text
//! [ERROR] chain 'rig:input-3' block 'rig:input-3:block:9211ce62-...'
//! uses pitch model 'native_pitch_shifter' with audio mode 'true_stereo'
//! that does not accept a mono input bus
//! ```
//!
//! Architecture invariant #5 (CLAUDE.md): every stream is ALWAYS stereo
//! internally — a mono physical input is broadcast to `Stereo([s, s])`
//! before the block chain. A model declaring `true_stereo` audio mode
//! therefore always receives a stereo bus, no matter how many physical
//! channels feed the chain. `validate_project` must not reject it.

use domain::ids::{BlockId, ChainId, DeviceId};
use project::block::{
    AudioBlock, AudioBlockKind, CoreBlock, InputBlock, InputEntry, OutputBlock, OutputEntry,
};
use project::chain::{Chain, ChainInputMode, ChainOutputMode};
use project::param::ParameterSet;
use project::project::Project;

use application::validate::validate_project;

const CHAIN_ID: &str = "rig:input-3";
const DEVICE: &str = "test:device";

fn mono_input() -> AudioBlock {
    AudioBlock {
        id: BlockId("rig:input-3:in".into()),
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

fn true_stereo_pitch_block() -> AudioBlock {
    // The user's exact block: pitch / native_pitch_shifter, which the
    // catalog registers with audio mode `true_stereo`.
    AudioBlock {
        id: BlockId("rig:input-3:block:9211ce62".into()),
        enabled: true,
        kind: AudioBlockKind::Core(CoreBlock {
            effect_type: "pitch".into(),
            model: "native_pitch_shifter".into(),
            params: ParameterSet::default(),
        }),
    }
}

fn stereo_output() -> AudioBlock {
    AudioBlock {
        id: BlockId("rig:input-3:out".into()),
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

#[test]
fn validate_accepts_true_stereo_pitch_on_mono_input_chain() {
    let project = Project {
        name: Some("test".into()),
        device_settings: Vec::new(),
        chains: vec![Chain {
            id: ChainId(CHAIN_ID.into()),
            description: None,
            instrument: "electric_guitar".into(),
            enabled: true,
            volume: 100.0,
            blocks: vec![mono_input(), true_stereo_pitch_block(), stereo_output()],
        }],
        midi: None,
    };

    let res = validate_project(&project);
    assert!(
        res.is_ok(),
        "a mono-input chain is broadcast to a stereo internal bus \
         (invariant #5), so a true_stereo pitch model must validate. \
         Got error: {:?}",
        res.err()
    );
}
