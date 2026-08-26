//! Responsibility: holds what the chain editor is editing.

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

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum ChainEditorMode {
    Create,
    Edit,
}
