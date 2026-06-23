use std::cell::RefCell;
use std::rc::Rc;

use crate::BlockEditorWindow;
use application::local_dispatcher::LocalDispatcher;
use project::chain::{ChainInputMode, ChainOutputMode};
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
    /// Dispatcher backed by the same `project` handle.
    pub(crate) dispatcher: Rc<LocalDispatcher>,
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
        let dispatcher = Rc::new(LocalDispatcher::new(Rc::clone(&project)));
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
        Self {
            project,
            dispatcher,
            project_path,
            config_path,
            presets_path,
            rig: None,
            io_bindings: Rc::new(RefCell::new(Vec::new())),
        }
    }
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
pub(crate) struct InputGroupDraft {
    pub(crate) device_id: Option<String>,
    pub(crate) channels: Vec<usize>,
    pub(crate) mode: ChainInputMode,
    /// I/O binding id — populated from `InputBlock.io` when the draft is built
    /// from an existing chain.  Used by the save path to dispatch
    /// `SaveChainInputEndpoints` instead of the legacy `SaveChain` stopgap.
    pub(crate) io: String,
    /// Endpoint name within the binding — populated from `InputBlock.endpoint`.
    pub(crate) endpoint: String,
}

#[derive(Debug, Clone)]
pub(crate) struct OutputGroupDraft {
    pub(crate) device_id: Option<String>,
    pub(crate) channels: Vec<usize>,
    pub(crate) mode: ChainOutputMode,
    /// I/O binding id — populated from `OutputBlock.io` when the draft is built
    /// from an existing chain.  Used by the save path to dispatch
    /// `SaveChainOutputEndpoints` instead of the legacy `SaveChain` stopgap.
    pub(crate) io: String,
    /// Endpoint name within the binding — populated from `OutputBlock.endpoint`.
    pub(crate) endpoint: String,
}

#[derive(Debug, Clone)]
pub(crate) struct ChainDraft {
    pub(crate) editing_index: Option<usize>,
    pub(crate) name: String,
    pub(crate) instrument: String,
    pub(crate) inputs: Vec<InputGroupDraft>,
    pub(crate) outputs: Vec<OutputGroupDraft>,
    pub(crate) editing_input_index: Option<usize>,
    pub(crate) editing_output_index: Option<usize>,
    /// Which block in chain.blocks is being edited by the I/O groups window.
    /// None = editing the fixed chip (first input / last output).
    /// Some(idx) = editing a specific I/O block at chain.blocks[idx].
    pub(crate) editing_io_block_index: Option<usize>,
    /// True when a new input entry was added as placeholder and the input config
    /// window is open. If the user cancels, the placeholder should be removed.
    pub(crate) adding_new_input: bool,
    /// True when a new output entry was added as placeholder and the output config
    /// window is open. If the user cancels, the placeholder should be removed.
    pub(crate) adding_new_output: bool,
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

/// Transient state for inserting an I/O block via the block type picker.
#[derive(Debug, Clone)]
pub(crate) struct IoBlockInsertDraft {
    pub(crate) chain_index: usize,
    pub(crate) before_index: usize,
    pub(crate) kind: String, // "input" or "output"
}

/// Transient state for editing an Insert block's send/return endpoints.
#[derive(Debug, Clone)]
pub(crate) struct InsertDraft {
    pub(crate) chain_index: usize,
    pub(crate) block_index: usize,
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
