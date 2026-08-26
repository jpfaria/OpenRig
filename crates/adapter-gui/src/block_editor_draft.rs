//! Responsibility: holds what the block editor is editing.

use project::param::ParameterSet;

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
    /// Registry binding id for the insert's external send/return loop (model A):
    /// the SEND goes out that binding's output, the RETURN comes back on its
    /// input. It is the whole editable state of an insert (#881).
    pub(crate) io: String,
}

/// #85 — the mid-chain I/O port being edited: which block it is and where it
/// points (E/S binding + endpoint). A port has no model and no parameters, so
/// this is its whole editable state.
#[derive(Clone)]
pub(crate) struct PortDraft {
    pub(crate) chain_index: usize,
    pub(crate) block_index: usize,
    /// `true` for an `Input` port, `false` for an `Output` one — it decides
    /// which side of the binding the endpoint list comes from.
    pub(crate) is_input: bool,
    pub(crate) io: String,
    pub(crate) endpoint: String,
    pub(crate) enabled: bool,
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
