//! #831 cross-transport parity: the payload a client gets must have the
//! same SHAPE whichever frontend served it — the GUI (which hosts live
//! sources) and a frontend that hosts nothing differ only in the VALUES
//! inside that shape, never in the fields, never in "this transport
//! refuses that read".
//!
//! This file drives every `QueryKind` twice through the real GUI resolver
//! and through `application::read::resolve` with `NoLiveSource`, and
//! compares the structure of both answers.

use std::cell::RefCell;
use std::rc::Rc;

use application::bridge::QueryKind;
use application::live_source::NoLiveSource;
use application::read::{resolve, ReadContext};
use domain::ids::{BlockId, ChainId};
use project::block::types::{AudioBlock, AudioBlockKind, InputBlock};
use project::chain::{Chain, LooperConfig, LooperSpeed};
use project::project::Project;
use serde_json::Value;
use slint::VecModel;

use super::QueryResolver;
use crate::spectrum_session::SpectrumSession;
use crate::state::ProjectSession;
use crate::tuner_session::TunerSession;
use crate::ProjectChainItem;

/// Every variant, so a new one cannot be added without deciding what it
/// answers on a frontend that hosts nothing.
fn all_kinds() -> Vec<QueryKind> {
    let chain = ChainId("guitar".to_string());
    vec![
        QueryKind::ProjectYaml,
        QueryKind::Devices,
        QueryKind::Ids,
        QueryKind::ChainMeters,
        QueryKind::TunerReadings,
        QueryKind::SpectrumReadings,
        QueryKind::DiLoopState,
        QueryKind::ChainLoopers {
            chain: chain.clone(),
        },
        QueryKind::ChainLatency {
            chain: chain.clone(),
        },
        QueryKind::ListChainPresets {
            chain: chain.clone(),
        },
        QueryKind::ListProjectPresets,
        QueryKind::ListPluginCatalog,
        QueryKind::GetPlugin {
            id: "x".to_string(),
        },
        QueryKind::FindPlugins {
            query: String::new(),
        },
        QueryKind::GetPluginParams {
            plugin_id: "x".to_string(),
        },
        QueryKind::GetBlockParams {
            chain: chain.clone(),
            block: BlockId("b".to_string()),
        },
        QueryKind::Paths,
        QueryKind::ChainQualityReport {
            chain: chain.clone(),
        },
        QueryKind::ChainToneReport { chain },
    ]
}

fn one_chain_project() -> Project {
    Project {
        name: Some("Parity".to_string()),
        device_settings: vec![],
        chains: vec![Chain {
            id: ChainId("guitar".to_string()),
            description: None,
            instrument: "guitar".to_string(),
            enabled: true,
            volume: 100.0,
            io_binding_ids: vec![],
            blocks: vec![AudioBlock {
                id: BlockId("b".to_string()),
                enabled: true,
                kind: AudioBlockKind::Input(InputBlock {
                    model: "default".to_string(),
                    io: String::new(),
                    endpoint: String::new(),
                }),
            }],
            di_output: None,
            // A persisted looper, so the looper read is exercised against a
            // chain that actually has one.
            loopers: vec![LooperConfig {
                uid: 1,
                mix: 0.8,
                decay: 0.5,
                speed: LooperSpeed::Normal,
                reverse: false,
                audio_file: None,
                input: None,
                output: None,
                preset: None,
            }],
        }],
        midi: None,
    }
}

/// The GUI-side live state a resolver reads from: chain rows carrying the
/// dBFS the IN/OUT bars are drawing this tick, and the analyzer/runtime
/// handles a headless test can hold (all empty — building them needs a real
/// audio device).
struct GuiState {
    _presets: tempfile::TempDir,
    session: ProjectSession,
    chain_rows: Rc<VecModel<ProjectChainItem>>,
    tuner: Rc<RefCell<Option<TunerSession>>>,
    spectrum: Rc<RefCell<Option<SpectrumSession>>>,
    runtime: Rc<RefCell<Option<infra_cpal::ProjectRuntimeController>>>,
}

impl GuiState {
    fn new(meters: (f32, f32)) -> Self {
        let presets = tempfile::tempdir().expect("temp presets dir");
        let session = ProjectSession::new(
            one_chain_project(),
            None,
            None,
            presets.path().to_path_buf(),
        );
        Self {
            _presets: presets,
            session,
            chain_rows: Rc::new(VecModel::from(vec![ProjectChainItem {
                meter_in_dbfs: meters.0,
                meter_out_dbfs: meters.1,
                ..Default::default()
            }])),
            tuner: Rc::new(RefCell::new(None)),
            spectrum: Rc::new(RefCell::new(None)),
            runtime: Rc::new(RefCell::new(None)),
        }
    }

    fn resolve(&self, kind: &QueryKind) -> Result<String, String> {
        QueryResolver {
            session: &self.session,
            chain_rows: &self.chain_rows,
            tuner: &self.tuner,
            spectrum: &self.spectrum,
            runtime: &self.runtime,
        }
        .resolve(kind)
    }
}

/// The same read on a frontend that hosts no live source at all.
fn resolve_unhosted(kind: &QueryKind) -> Result<String, String> {
    let project = one_chain_project();
    let dispatcher =
        application::local_dispatcher::LocalDispatcher::new(Rc::new(RefCell::new(project.clone())));
    resolve(
        kind,
        &ReadContext {
            project: &project,
            rig: None,
            io_bindings: &[],
            dispatcher: &dispatcher,
            live: &NoLiveSource,
        },
    )
}

/// Do two payloads carry the same structure?
///
/// Values are erased; only fields and their types are compared. Two
/// deliberate allowances, both of them documented empty shapes rather than
/// shape differences: an EMPTY array matches an array of any element (an
/// unhosted analyzer reports `"rows":[]`), and `null` matches any value (a
/// nullable field such as the DI's `source` reads null when nothing is
/// loaded).
fn json_shapes_match(a: &Value, b: &Value) -> bool {
    match (a, b) {
        (Value::Array(x), Value::Array(y)) => match (x.first(), y.first()) {
            (Some(xa), Some(ya)) => json_shapes_match(xa, ya),
            _ => true,
        },
        (Value::Object(x), Value::Object(y)) => {
            x.len() == y.len()
                && x.iter()
                    .all(|(k, xv)| y.get(k).is_some_and(|yv| json_shapes_match(xv, yv)))
        }
        (Value::Null, _) | (_, Value::Null) => true,
        _ => std::mem::discriminant(a) == std::mem::discriminant(b),
    }
}

/// Tab-separated field count per line — the structure of the line-oriented
/// payloads (chain meters, ids, the device listing).
fn text_shape(s: &str) -> Vec<usize> {
    s.lines().map(|l| l.split('\t').count()).collect()
}

fn payload_shapes_match(a: &str, b: &str) -> bool {
    match (
        serde_json::from_str::<Value>(a),
        serde_json::from_str::<Value>(b),
    ) {
        (Ok(x), Ok(y)) => json_shapes_match(&x, &y),
        // An empty listing is the documented empty shape of a line-oriented
        // payload, not a different shape.
        _ => a.is_empty() || b.is_empty() || text_shape(a) == text_shape(b),
    }
}

#[test]
fn every_query_kind_answers_with_the_same_shape_hosted_and_unhosted() {
    let gui = GuiState::new((-12.0, -6.0));
    for kind in all_kinds() {
        // `Devices` is the one read whose hosted answer needs a real audio
        // host (`infra_cpal::list_devices`), so it cannot run in a headless
        // unit test. Its hosted/unhosted parity is pinned at the resolver,
        // in `application`'s `read_tests.rs`.
        if matches!(kind, QueryKind::Devices) {
            continue;
        }
        let hosted = gui.resolve(&kind);
        let unhosted = resolve_unhosted(&kind);
        assert_eq!(
            hosted.is_err(),
            unhosted.is_err(),
            "{kind:?}: one frontend refuses a read the other answers — \
             hosted={hosted:?} unhosted={unhosted:?}"
        );
        let (hosted, unhosted) = match (hosted, unhosted) {
            (Ok(h), Ok(u)) => (h, u),
            (Err(h), Err(u)) => (h, u),
            _ => unreachable!("checked above"),
        };
        assert!(
            payload_shapes_match(&hosted, &unhosted),
            "{kind:?}: the payload shape depends on which frontend served it — \
             hosted={hosted} unhosted={unhosted}"
        );
    }
}

#[test]
fn the_hosted_answer_carries_the_live_values_inside_that_shape() {
    // Same shape is only half the contract: the GUI must actually report
    // what its meters are showing, not the silent shape.
    let gui = GuiState::new((-12.0, -6.0));
    let hosted = gui
        .resolve(&QueryKind::ChainMeters)
        .expect("the GUI hosts meters");
    assert_eq!(hosted, "guitar\t-12.0\t-6.0\n");
}
