//! #808 — the Tone Doctor's "apply suggestion" must re-sync the chain's live
//! runtime, exactly like every other parameter surface.
//!
//! The owner's report: monitoring a DI, applying a doctor fix (or changing an
//! amp param) changed NOTHING until the block was toggled off/on. Root cause:
//! `apply_cached_suggestion` dispatched the parameter command but never called
//! `sync_live_chain_runtime`, so the live runtime was never rebuilt and the
//! monitored DI (a dedicated pre-render) kept playing the stale render. A block
//! toggle took a different path that DID re-arm the DI, which is why only the
//! toggle "applied" the value.
//!
//! This drives the windowless core with a spy sync so the contract is pinned
//! without an `AppWindow` or a real audio device: applying a suggestion MUST
//! request a live-runtime re-sync for the edited chain.

use std::cell::RefCell;
use std::rc::Rc;
use std::sync::{Arc, Mutex};

use domain::ids::{BlockId, ChainId};
use domain::value_objects::ParameterValue;
use engine::tone_doctor_suggestion::Suggestion;
use project::block::{schema_for_block_model, AudioBlock, AudioBlockKind, CoreBlock};
use project::chain::Chain;
use project::param::ParameterSet;
use project::project::Project;

use super::apply_cached_suggestion;
use crate::state::ProjectSession;

const BLOCK_ID: &str = "doctor808:gain";
const CHAIN_ID: &str = "doctor808-chain";

fn init() {
    use std::sync::Once;
    static I: Once = Once::new();
    I.call_once(|| engine::native_registry::register_all_natives());
}

fn gain_params(volume_pct: f32) -> ParameterSet {
    let schema = schema_for_block_model("gain", "volume").expect("volume schema must exist");
    let mut ps = ParameterSet::default();
    ps.insert("volume", ParameterValue::Float(volume_pct));
    ps.normalized_against(&schema)
        .expect("volume param must normalize")
}

fn session_with_gain_chain(volume_pct: f32) -> ProjectSession {
    let project = Project {
        name: None,
        device_settings: vec![],
        chains: vec![Chain {
            id: ChainId(CHAIN_ID.into()),
            description: None,
            instrument: "electric_guitar".into(),
            enabled: true,
            volume: 100.0,
            io_binding_ids: vec![],
            blocks: vec![AudioBlock {
                id: BlockId(BLOCK_ID.into()),
                enabled: true,
                kind: AudioBlockKind::Core(CoreBlock {
                    effect_type: "gain".into(),
                    model: "volume".into(),
                    params: gain_params(volume_pct),
                }),
            }],
            di_output: None,
        }],
        midi: None,
    };
    // A temp presets path the test never writes to (no SaveProject dispatched).
    ProjectSession::new(project, None, None, std::env::temp_dir().join("openrig-808-tests"))
}

/// Set the gain's `volume` far from its current value so the fix is unmistakable.
fn suggestion() -> Suggestion {
    Suggestion {
        block_index: 0,
        param_path: "volume".into(),
        param_label: "Volume".into(),
        current: 20.0,
        suggested: 90.0,
        enable_path: None,
        rationale: String::new(),
    }
}

#[test]
fn applying_a_suggestion_re_syncs_the_live_runtime() {
    init();
    let session = session_with_gain_chain(20.0);
    let cache = Arc::new(Mutex::new(Some(suggestion())));

    // Spy: record every chain the apply asks to re-sync.
    let synced: Rc<RefCell<Vec<ChainId>>> = Rc::new(RefCell::new(Vec::new()));
    let synced_probe = synced.clone();
    apply_cached_suggestion(&session, 0, &cache, |chain_id| {
        synced_probe.borrow_mut().push(chain_id.clone());
        Ok(())
    })
    .expect("apply must not error");

    assert_eq!(
        synced.borrow().as_slice(),
        &[ChainId(CHAIN_ID.into())],
        "#808: applying a Tone Doctor suggestion must re-sync the edited chain's \
         live runtime (so the change is audible on the DI at once) — it dispatched \
         the parameter but never requested a re-sync, so the sound only changed \
         after a block toggle."
    );
}
