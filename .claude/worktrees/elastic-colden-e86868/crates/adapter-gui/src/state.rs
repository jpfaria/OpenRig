use crate::BlockEditorWindow;
use project::chain::{ChainInputMode, ChainOutputMode};
use project::param::ParameterSet;
use project::project::Project;
use serde::{Deserialize, Serialize};
use slint::Timer;
use std::path::PathBuf;
use std::rc::Rc;

#[derive(Debug, Clone)]
pub(crate) struct ProjectPaths {
    pub(crate) default_config_path: PathBuf,
}

#[derive(Debug, Deserialize, Serialize, Default)]
pub(crate) struct AppConfigYaml {
    #[serde(default)]
    pub(crate) presets_path: Option<PathBuf>,
}

#[derive(Debug, Clone)]
pub(crate) struct ProjectSession {
    pub(crate) project: Project,
    pub(crate) project_path: Option<PathBuf>,
    pub(crate) config_path: Option<PathBuf>,
    pub(crate) presets_path: PathBuf,
}

#[derive(Debug, Clone)]
pub(crate) struct InputGroupDraft {
    pub(crate) device_id: Option<String>,
    pub(crate) channels: Vec<usize>,
    pub(crate) mode: ChainInputMode,
}

#[derive(Debug, Clone)]
pub(crate) struct OutputGroupDraft {
    pub(crate) device_id: Option<String>,
    pub(crate) channels: Vec<usize>,
    pub(crate) mode: ChainOutputMode,
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

#[derive(Debug, Serialize)]
pub(crate) struct ConfigYaml {
    pub(crate) presets_path: String,
}

pub(crate) const UNTITLED_PROJECT_NAME: &str = "UNTITLED PROJECT";

pub(crate) struct BlockWindow {
    pub(crate) chain_index: usize,
    pub(crate) block_index: usize,
    pub(crate) window: BlockEditorWindow,
    #[allow(dead_code)]
    pub(crate) stream_timer: Option<Rc<Timer>>,
}
