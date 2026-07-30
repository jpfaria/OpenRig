use std::cell::RefCell;
use std::rc::Rc;

use crate::BlockEditorWindow;
use application::dispatcher::CommandDispatcher;
use application::local_dispatcher::LocalDispatcher;
use application::metronome_state::MetronomeControlState;
use infra_filesystem::FilesystemStorage;
use project::chain::ChainInputMode;
use project::param::ParameterSet;
use project::project::Project;
use project::rig::RigProject;
use serde::{Deserialize, Serialize};
use slint::Timer;
use std::path::PathBuf;

#[derive(Debug, Clone)]
pub(crate) struct ProjectPaths {
    pub(crate) default_config_path: PathBuf,
}

#[derive(Debug, Deserialize, Serialize, Default)]
pub(crate) struct AppConfigYaml {
    #[serde(default)]
    pub(crate) presets_path: Option<PathBuf>,
}

#[derive(Clone)]
pub(crate) struct ProjectSession {
    /// The project data, shared with the `LocalDispatcher` so both sides
    /// operate on the same allocation with no sync step.
    pub(crate) project: Rc<RefCell<Project>>,
    /// The command bus this session talks to. #127: held as a trait object so
    /// the frontend never names an implementation — `new` supplies a
    /// `LocalDispatcher`, a remote transport can supply its own.
    pub(crate) dispatcher: Rc<dyn CommandDispatcher>,
    pub(crate) project_path: Option<PathBuf>,
    pub(crate) config_path: Option<PathBuf>,
    pub(crate) presets_path: PathBuf,
    /// The rig the synthetic `project` was projected from (#436). Kept so
    /// the legacy chains screen can switch preset/scene per input. `None`
    /// for sessions not loaded through the rig path (e.g. brand-new).
    pub(crate) rig: Option<Rc<RefCell<RigProject>>>,
    /// Issue #716 — the per-machine I/O binding registry (`AppConfig.io_bindings`)
    /// the runtime controller resolves bound chains against. Mirrored from
    /// `AppConfig` when the session is created and refreshed on config edits;
    /// the sync helpers push it into the controller before each (re)build.
    pub(crate) io_bindings: Rc<RefCell<Vec<infra_filesystem::IoBinding>>>,
}

impl ProjectSession {
    /// Create a new session from an owned `Project`.
    ///
    /// Both `self.project` and `self.dispatcher` share the same
    /// `Rc<RefCell<Project>>` handle.
    pub(crate) fn new(
        project: Project,
        project_path: Option<PathBuf>,
        config_path: Option<PathBuf>,
        presets_path: PathBuf,
    ) -> Self {
        let project = Rc::new(RefCell::new(project));
        let dispatcher =
            Rc::new(LocalDispatcher::new(Rc::clone(&project))) as Rc<dyn CommandDispatcher>;
        Self::assemble(project, dispatcher, project_path, config_path, presets_path)
    }

    /// Build a session around a dispatcher the caller chose. [`Self::new`] is
    /// this with a `LocalDispatcher`; tests and future transports use it
    /// directly.
    ///
    /// Unlike `new`, the `project` handle is NOT shared with the dispatcher —
    /// a non-local dispatcher owns its own state, so the caller passes the
    /// projection the frontend reads.
    #[cfg_attr(not(test), allow(dead_code))]
    pub(crate) fn with_dispatcher(
        project: Project,
        dispatcher: Rc<dyn CommandDispatcher>,
        project_path: Option<PathBuf>,
        config_path: Option<PathBuf>,
        presets_path: PathBuf,
    ) -> Self {
        Self::assemble(
            Rc::new(RefCell::new(project)),
            dispatcher,
            project_path,
            config_path,
            presets_path,
        )
    }

    /// Shared tail of both constructors: attach the resolved paths, then build
    /// the struct.
    fn assemble(
        project: Rc<RefCell<Project>>,
        dispatcher: Rc<dyn CommandDispatcher>,
        project_path: Option<PathBuf>,
        config_path: Option<PathBuf>,
        presets_path: PathBuf,
    ) -> Self {
        // #555: the dispatcher owns the file I/O for SaveProject /
        // SaveChainPreset / DeleteChainPreset so MCP / MIDI / GUI all
        // hit the same disk locations. Attach the session's resolved
        // paths right after construction; the file dialog wiring
        // re-attaches them on "Save As" if the user picks a new path.
        dispatcher.attach_presets_path(presets_path.clone());
        if let Some(ref path) = project_path {
            dispatcher.attach_project_path(path.clone());
        }
        dispatcher.attach_config_path(config_path.clone());
        // #127: the binding commands must mutate the SAME registry this
        // session renders from and re-installs into the controller on every
        // runtime sync — otherwise an edit issued off the GUI is reverted at
        // the next sync.
        let io_bindings = Rc::new(RefCell::new(Vec::new()));
        dispatcher.attach_io_bindings(Rc::clone(&io_bindings));
        attach_metronome_state(dispatcher.as_ref());
        Self {
            project,
            dispatcher,
            project_path,
            config_path,
            presets_path,
            rig: None,
            io_bindings,
        }
    }
}

/// #127/#14: give the session's dispatcher the metronome state it owns —
/// the settings restored from the per-machine `config.yaml` (ADR 0003), the
/// endpoint they were last played through, and the file they persist back to.
///
/// Every session gets one, however it was built, because the dispatcher is
/// where a metronome command is answered now: the GUI knob, a MIDI footswitch
/// and an MCP client all reach the same settings and the same click.
///
/// The path is resolved HERE, on the thread that will dispatch, and travels
/// with the state — the persist worker never re-resolves it (#701).
fn attach_metronome_state(dispatcher: &dyn CommandDispatcher) {
    let config = FilesystemStorage::load_app_config().unwrap_or_default();
    dispatcher.attach_metronome_state(Rc::new(RefCell::new(MetronomeControlState::restored(
        &config.metronome,
        metronome_config_path(),
    ))));
}

/// Where the metronome's settings persist: the machine's `config.yaml`.
///
/// **`None` in a test build, structurally** (#701). A session dispatching
/// `SetMetronomeBpm` writes this file, and a test that built a `ProjectSession`
/// would therefore rewrite the tempo, bar and output the person running the
/// suite actually practises with — it did, once, before this guard existed. A
/// state with no path persists nothing, so no test CAN reach the real config
/// however it builds its session. `ProjectSession` is `pub(crate)`, so an
/// integration test cannot construct one and slip past the `cfg`.
#[cfg(not(test))]
fn metronome_config_path() -> Option<PathBuf> {
    FilesystemStorage::app_config_path().ok()
}

#[cfg(test)]
fn metronome_config_path() -> Option<PathBuf> {
    None
}

impl std::fmt::Debug for ProjectSession {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("ProjectSession")
            .field("project", &self.project.borrow().name)
            .field("project_path", &self.project_path)
            .field("config_path", &self.config_path)
            .field("presets_path", &self.presets_path)
            .finish()
    }
}

#[derive(Debug, Clone)]
pub(crate) struct ChainDraft {
    pub(crate) editing_index: Option<usize>,
    pub(crate) name: String,
    pub(crate) instrument: String,
    /// #716: the I/O bindings this chain selects (checklist). The chain's
    /// input/output is discovered from these; the legacy per-endpoint I/O
    /// editor was removed.
    pub(crate) io_binding_ids: Vec<String>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct SelectedBlock {
    pub(crate) chain_index: usize,
    pub(crate) block_index: usize,
}

#[derive(Debug, Clone)]
pub(crate) struct BlockEditorDraft {
    pub(crate) chain_index: usize,
    pub(crate) block_index: Option<usize>,
    pub(crate) before_index: usize,
    pub(crate) instrument: String,
    pub(crate) effect_type: String,
    pub(crate) model_id: String,
    pub(crate) enabled: bool,
    pub(crate) is_select: bool,
}

/// Transient state for editing an Insert block.
///
/// #716 (model A): an insert references ONE I/O binding (`io`); the send goes to
/// that binding's output and the return comes from its input, both resolved from
/// the per-machine registry. The legacy `send_*`/`return_*` device-picker fields
/// are kept ONLY to back the existing Slint device/channel widgets until the
/// insert editor is reworked to pick a binding (see TODO(#716) in
/// `insert_wiring.rs`). They are no longer persisted onto the block.
#[derive(Debug, Clone)]
pub(crate) struct InsertDraft {
    pub(crate) chain_index: usize,
    pub(crate) block_index: usize,
    /// Registry binding id for the insert's external send/return loop (model A).
    pub(crate) io: String,
    pub(crate) send_device_id: Option<String>,
    pub(crate) send_channels: Vec<usize>,
    pub(crate) send_mode: ChainInputMode,
    pub(crate) return_device_id: Option<String>,
    pub(crate) return_channels: Vec<usize>,
    pub(crate) return_mode: ChainInputMode,
}

pub(crate) struct BlockEditorData {
    pub(crate) effect_type: String,
    pub(crate) model_id: String,
    pub(crate) params: ParameterSet,
    pub(crate) enabled: bool,
    pub(crate) is_select: bool,
    pub(crate) select_options: Vec<SelectOptionEditorItem>,
    pub(crate) selected_select_option_block_id: Option<String>,
}

pub(crate) struct SelectOptionEditorItem {
    pub(crate) block_id: String,
    pub(crate) label: String,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum ChainEditorMode {
    Create,
    Edit,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum AudioSettingsMode {
    Gui,
    Project,
}

// `ConfigYaml` moved into `Command::SaveProject`'s dispatcher
// handler (`application::local_dispatcher_project`) in #555 — the
// sidecar `config.yaml` is now written there with a fixed
// `presets_path: ./presets` body, matching what this struct produced.

pub(crate) const UNTITLED_PROJECT_NAME: &str = "UNTITLED PROJECT";

pub(crate) struct BlockWindow {
    pub(crate) chain_index: usize,
    pub(crate) block_index: usize,
    pub(crate) window: BlockEditorWindow,
    #[allow(dead_code)]
    pub(crate) stream_timer: Option<Rc<Timer>>,
}

#[cfg(test)]
#[path = "state_dyn_dispatcher_tests.rs"]
mod dyn_dispatcher_tests;
