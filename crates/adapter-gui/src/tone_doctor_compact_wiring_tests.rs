//! #808 — the Tone Doctor's "apply fix" must re-sync the chain's live runtime,
//! exactly like every other parameter surface.
//!
//! The owner's report: monitoring a DI, applying a doctor fix (or changing an
//! amp param) changed NOTHING until the block was toggled off/on. Root cause:
//! the apply dispatched the parameter command but never called
//! `sync_live_chain_runtime`, so the live runtime was never rebuilt and the
//! monitored DI (a dedicated pre-render) kept playing the stale render. A block
//! toggle took a different path that DID re-arm the DI, which is why only the
//! toggle "applied" the value.
//!
//! This drives the windowless core with a spy sync so the contract is pinned
//! without an `AppWindow` or a real audio device: applying a fix MUST request a
//! live-runtime re-sync for the edited chain.
//!
//! #791 (reopen): the fix now comes from `Command::ApplyToneDoctorFix`, which
//! reads the verdict the dispatcher cached — so the test runs a real diagnosis
//! through the bus first, the same way the panel does.

use std::cell::RefCell;
use std::rc::Rc;

use application::command::{Command, ToneDoctorCommand};
use application::dispatcher::CommandDispatcher;
use application::event::Event;
use domain::ids::{BlockId, ChainId};
use domain::value_objects::ParameterValue;
use project::block::{schema_for_block_model, AudioBlock, AudioBlockKind, CoreBlock};
use project::chain::Chain;
use project::param::ParameterSet;
use project::project::Project;

use super::apply_fix_inner;
use crate::state::ProjectSession;

const CHAIN_ID: &str = "doctor808-chain";
const SR: f32 = 48_000.0;

fn init() {
    use std::sync::Once;
    static I: Once = Once::new();
    I.call_once(|| engine::native_registry::register_all_natives());
}

fn params(model: &str, values: &[(&str, f32)]) -> ParameterSet {
    let schema = schema_for_block_model("gain", model).expect("gain schema must exist");
    let mut ps = ParameterSet::default();
    for (k, v) in values {
        ps.insert(*k, ParameterValue::Float(*v));
    }
    ps.normalized_against(&schema).expect("params normalize")
}

fn block(id: &str, model: &str, ps: ParameterSet) -> AudioBlock {
    AudioBlock {
        id: BlockId(id.into()),
        enabled: true,
        kind: AudioBlockKind::Core(CoreBlock {
            effect_type: "gain".into(),
            model: model.into(),
            params: ps,
        }),
    }
}

/// A chain that measurably fizzes, so the diagnosis yields a fix to apply.
fn session_with_fizzy_chain() -> ProjectSession {
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
            blocks: vec![
                block("vol", "volume", params("volume", &[("volume", 80.0)])),
                block(
                    "fz",
                    "fuzz_si",
                    params(
                        "fuzz_si",
                        &[("fuzz", 95.0), ("tone", 90.0), ("level", 50.0)],
                    ),
                ),
            ],
            di_output: None,
        }],
        midi: None,
    };
    // A temp presets path the test never writes to (no SaveProject dispatched).
    ProjectSession::new(
        project,
        None,
        None,
        std::env::temp_dir().join("openrig-808-tests"),
    )
}

fn sine() -> Vec<[f32; 2]> {
    (0..SR as usize)
        .map(|i| {
            let s = 0.5 * (2.0 * std::f32::consts::PI * 1_000.0 * i as f32 / SR).sin();
            [s, s]
        })
        .collect()
}

/// Run a diagnosis through the bus and wait for the verdict to land.
fn diagnose(session: &ProjectSession) {
    session
        .dispatcher
        .attach_tone_doctor_input(Box::new(|_chain, _seconds| {
            Some(Box::new(|| Some((sine(), SR))))
        }));
    session
        .dispatcher
        .dispatch(Command::ToneDoctor(ToneDoctorCommand::DiagnoseChainTone {
            chain: ChainId(CHAIN_ID.into()),
            genre: None,
            seconds: None,
        }))
        .expect("diagnose dispatches");

    for _ in 0..600 {
        let diagnosed = session
            .dispatcher
            .poll_async_results()
            .iter()
            .any(|e| matches!(e, Event::ChainToneDiagnosed { .. }));
        if diagnosed {
            return;
        }
        std::thread::sleep(std::time::Duration::from_millis(50));
    }
    panic!("the diagnosis never completed");
}

#[test]
fn applying_a_fix_re_syncs_the_live_runtime() {
    init();
    let session = session_with_fizzy_chain();
    diagnose(&session);

    // Spy: record every chain the apply asks to re-sync.
    let synced: Rc<RefCell<Vec<ChainId>>> = Rc::new(RefCell::new(Vec::new()));
    let synced_probe = synced.clone();
    apply_fix_inner(&session, 0, |chain_id| {
        synced_probe.borrow_mut().push(chain_id.clone());
        Ok(())
    })
    .expect("apply must not error");

    assert_eq!(
        synced.borrow().as_slice(),
        &[ChainId(CHAIN_ID.into())],
        "#808: applying a Tone Doctor fix must re-sync the edited chain's live \
         runtime (so the change is audible on the DI at once) — it dispatched \
         the parameter but never requested a re-sync, so the sound only changed \
         after a block toggle."
    );
}
