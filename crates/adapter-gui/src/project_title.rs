//! Responsibility: writes the title bar text for the open project.

use project::project::Project;
use std::path::PathBuf;

pub(crate) fn project_title_for_path(project_path: Option<&PathBuf>, project: &Project) -> String {
    if let Some(name) = project
        .name
        .as_ref()
        .map(|name| name.trim())
        .filter(|name| !name.is_empty())
    {
        return name.to_string();
    }
    project_path
        .and_then(|path| path.file_stem())
        .and_then(|name| name.to_str())
        .map(|name| name.to_string())
        .unwrap_or_else(|| {
            if project.chains.is_empty() {
                "Novo Projeto".to_string()
            } else {
                "Projeto".to_string()
            }
        })
}

#[cfg(test)]
#[path = "project_ops_persistence_tests.rs"]
mod project_ops_persistence_tests;

#[cfg(test)]
#[path = "project_ops_persistence_more_tests.rs"]
mod project_ops_persistence_more;

#[cfg(test)]
#[path = "project_admin_persistence_tests.rs"]
mod project_admin_persistence_tests;

#[cfg(test)]
#[path = "project_admin_nam_tests.rs"]
mod project_admin_nam;

#[cfg(test)]
#[path = "project_rig_persistence_tests.rs"]
mod project_rig_persistence_tests;

#[cfg(test)]
#[path = "project_chain_defaults_persistence_tests.rs"]
mod project_chain_defaults_persistence_tests;

#[cfg(test)]
#[path = "project_chain_inmemory_tests.rs"]
mod project_chain_inmemory_tests;

#[cfg(test)]
#[path = "chain_rename_persistence_tests.rs"]
mod chain_rename_persistence_tests;

#[cfg(test)]
#[path = "scene_param_persistence_tests.rs"]
mod scene_param_persistence_tests;

#[cfg(test)]
#[path = "issue_690_nam_gate_persistence_tests.rs"]
mod issue_690_nam_gate_persistence_tests;
