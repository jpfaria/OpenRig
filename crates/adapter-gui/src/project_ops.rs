//! Responsibility: keeps the historical `project_ops` path pointing at the six things it held.
//!
//! It was responsible for loading the machine config, resolving the project
//! paths, opening a session, tracking the dirty flag, translating the
//! screen's device choices, and the title bar text (#873).

pub(crate) use crate::app_config_load::{load_and_sync_app_config, resolve_project_config_path};
pub(crate) use crate::gui_device_settings::build_device_settings_from_gui;
#[cfg(test)]
pub(crate) use crate::project_dirty::dirty_snapshot;
#[cfg(test)]
pub(crate) use crate::project_dirty::save_project_session;
pub(crate) use crate::project_dirty::{
    project_session_snapshot, set_project_dirty, sync_project_dirty,
};
pub(crate) use crate::project_paths_resolve::resolve_project_paths;
#[cfg(test)]
pub(crate) use crate::project_session_load::load_rig_and_project;
pub(crate) use crate::project_session_load::{
    create_new_project_session, load_preset_file, load_project_session, open_cli_project,
};
pub(crate) use crate::project_title::project_title_for_path;

// Issue #792 split: recent-projects + path/name helpers live in
// project_ops_recents.rs. Re-exported so crate::project_ops::* and the
// super:: paths in the persistence test modules keep resolving.
#[cfg(test)]
pub(crate) use crate::project_ops_recents::sync_recent_projects;
pub(crate) use crate::project_ops_recents::{
    canonical_project_path, mark_recent_project_invalid, project_display_name,
    recent_project_items, register_recent_project,
};

#[cfg(test)]
#[path = "chain_reorder_refresh_tests.rs"]
mod chain_reorder_refresh_tests;
