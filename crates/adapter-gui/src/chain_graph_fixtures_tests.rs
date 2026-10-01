//! #328 — fixtures shared by the chain-graph tests. Test-only module.

use std::cell::RefCell;
use std::path::PathBuf;
use std::rc::Rc;
use std::sync::{Arc, RwLock};

use application::command::Command;
use application::dispatcher::CommandDispatcher;
use application::event::Event;
use application::selection_state::SelectionState;
use domain::ids::{BlockId, ChainId, DeviceId};
use domain::io_binding::ChannelMode;
use infra_filesystem::{IoBinding, IoEndpoint};
use project::block::{AudioBlock, AudioBlockKind, CoreBlock, InputBlock, SplitBlock, SplitEnd};
use project::chain::Chain;
use project::project::Project;
use slint::VecModel;

use crate::state::ProjectSession;
use crate::ProjectChainItem;

/// The split node of the split `sp` that `mix_chain` and `y_chain` carry.
pub(crate) const FIRST_SPLIT_NODE_ID: &str = "__split_sp";
/// The mixer node of `mix_chain`'s split `sp`.
pub(crate) const FIRST_MIXER_NODE_ID: &str = "__merge_sp";

/// A native gain/volume block — a real catalog model
/// (`crates/block-gain/src/native_volume.rs`, param `volume` 0..100).
pub(crate) fn core(id: &str) -> AudioBlock {
    AudioBlock {
        id: BlockId(id.into()),
        enabled: true,
        kind: AudioBlockKind::Core(CoreBlock {
            effect_type: "gain".into(),
            model: "volume".into(),
            params: Default::default(),
        }),
    }
}

/// A mid `Input` port reading `endpoint` of binding `io`.
pub(crate) fn port_in(id: &str, io: &str, endpoint: &str) -> AudioBlock {
    AudioBlock {
        id: BlockId(id.into()),
        enabled: true,
        kind: AudioBlockKind::Input(InputBlock {
            model: "standard".into(),
            io: io.into(),
            endpoint: endpoint.into(),
        }),
    }
}

pub(crate) fn split(id: &str, end: SplitEnd, a: Vec<AudioBlock>, b: Vec<AudioBlock>) -> AudioBlock {
    split_paths(id, end, vec![a, b])
}

/// A split with any number of paths.
pub(crate) fn split_paths(id: &str, end: SplitEnd, paths: Vec<Vec<AudioBlock>>) -> AudioBlock {
    AudioBlock {
        id: BlockId(id.into()),
        enabled: true,
        kind: AudioBlockKind::Split(SplitBlock::with_paths(end, paths)),
    }
}

pub(crate) fn chain(blocks: Vec<AudioBlock>) -> Chain {
    Chain {
        mix: Default::default(),
        id: ChainId("chain:0".into()),
        description: None,
        instrument: "electric_guitar".into(),
        enabled: false,
        volume: 100.0,
        io_binding_ids: vec!["main".into(), "aux".into()],
        blocks,
        di_output: None,
        loopers: vec![],
        disabled_endpoints: Default::default(),
    }
}

/// `pre → split sp (A: a1, a2 | B: b1) → post`, the lanes summed by the mixer.
pub(crate) fn mix_chain() -> Chain {
    chain(vec![
        core("pre"),
        split(
            "sp",
            SplitEnd::Mix,
            vec![core("a1"), core("a2")],
            vec![core("b1")],
        ),
        core("post"),
    ])
}

/// `pre → split sp (A: a1 | B: b1)`, each lane to its own outputs.
pub(crate) fn y_chain() -> Chain {
    chain(vec![
        core("pre"),
        split("sp", SplitEnd::Y, vec![core("a1")], vec![core("b1")]),
    ])
}

/// `pre → Mix mx (A: ma | B: mb) → mid → Y y (A: ya | B: yb)`: the one Mix,
/// then the one Y as the last processing block.
pub(crate) fn mix_then_y_chain() -> Chain {
    chain(vec![
        core("pre"),
        split("mx", SplitEnd::Mix, vec![core("ma")], vec![core("mb")]),
        core("mid"),
        split("y", SplitEnd::Y, vec![core("ya")], vec![core("yb")]),
    ])
}

pub(crate) fn endpoint(name: &str) -> IoEndpoint {
    IoEndpoint {
        name: name.into(),
        device_id: DeviceId("dev".into()),
        mode: ChannelMode::Stereo,
        channels: vec![0, 1],
    }
}

/// `main` ("Scarlett"): inputs In 1, In 2 — output Out L/R.
/// `aux` ("AUX"): output Out L/R (same endpoint name as main's).
pub(crate) fn registry() -> Vec<IoBinding> {
    vec![
        IoBinding {
            id: "main".into(),
            name: "Scarlett".into(),
            inputs: vec![endpoint("In 1"), endpoint("In 2")],
            outputs: vec![endpoint("Out L/R")],
        },
        IoBinding {
            id: "aux".into(),
            name: "AUX".into(),
            inputs: vec![],
            outputs: vec![endpoint("Out L/R")],
        },
    ]
}

fn project(chains: Vec<Chain>) -> Project {
    Project {
        name: None,
        device_settings: vec![],
        chains,
        midi: None,
    }
}

/// A session on the real `LocalDispatcher` (Part 2 applies the commands).
pub(crate) fn session_with(chains: Vec<Chain>) -> Rc<RefCell<Option<ProjectSession>>> {
    let session = ProjectSession::new(
        project(chains),
        None,
        None,
        std::env::temp_dir().join("openrig-328-graph-tests"),
    );
    *session.io_bindings.borrow_mut() = registry();
    Rc::new(RefCell::new(Some(session)))
}

/// Records every command the GUI dispatched; applies nothing.
pub(crate) struct RecordingDispatcher {
    pub(crate) seen: RefCell<Vec<Command>>,
    selection: Arc<RwLock<SelectionState>>,
}

impl CommandDispatcher for RecordingDispatcher {
    fn dispatch(&self, cmd: Command) -> anyhow::Result<Vec<Event>> {
        self.seen.borrow_mut().push(cmd);
        Ok(Vec::new())
    }
    fn selection_state(&self) -> Arc<RwLock<SelectionState>> {
        Arc::clone(&self.selection)
    }
}

pub(crate) fn recording_session(
    chains: Vec<Chain>,
) -> (Rc<RefCell<Option<ProjectSession>>>, Rc<RecordingDispatcher>) {
    let recorder = Rc::new(RecordingDispatcher {
        seen: RefCell::new(Vec::new()),
        selection: Arc::new(RwLock::new(SelectionState::default())),
    });
    let session = ProjectSession::with_dispatcher(
        project(chains),
        Rc::clone(&recorder) as Rc<dyn CommandDispatcher>,
        None,
        None,
        PathBuf::from("presets"),
    );
    *session.io_bindings.borrow_mut() = registry();
    (Rc::new(RefCell::new(Some(session))), recorder)
}

pub(crate) fn rows() -> Rc<VecModel<ProjectChainItem>> {
    // Republishing the rows walks the asset paths, which panic until startup
    // set them (same guard as `block_delete_tests.rs:70-75`).
    infra_filesystem::init_asset_paths(infra_filesystem::AssetPaths::default());
    Rc::new(VecModel::from(Vec::<ProjectChainItem>::new()))
}

pub(crate) fn chain_in(session: &Rc<RefCell<Option<ProjectSession>>>, index: usize) -> Chain {
    session.borrow().as_ref().unwrap().project.borrow().chains[index].clone()
}
