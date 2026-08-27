//! Responsibility: keeps the historical `state` path pointing at the five things it held.
//!
//! It was responsible for the open project, the chain editor's draft, the
//! block editor's draft, the audio settings mode and the detached block
//! windows (#873).

pub(crate) use crate::audio_settings_mode::AudioSettingsMode;
pub(crate) use crate::block_editor_draft::{
    BlockEditorData, BlockEditorDraft, InsertDraft, PortDraft, SelectOptionEditorItem,
    SelectedBlock,
};
pub(crate) use crate::block_window::BlockWindow;
pub(crate) use crate::chain_draft::{ChainDraft, ChainEditorMode};
pub(crate) use crate::project_session::{
    AppConfigYaml, ProjectPaths, ProjectSession, UNTITLED_PROJECT_NAME,
};

// `state_dyn_dispatcher_tests.rs` hangs off this module and builds a project
// through `super::`, as it did before the split (#873).
#[cfg(test)]
pub(crate) use project::project::Project;

#[cfg(test)]
#[path = "state_dyn_dispatcher_tests.rs"]
mod dyn_dispatcher_tests;
