//! Responsibility: names a project for the screen.

use project::project::Project;

use crate::UNTITLED_PROJECT_NAME;

pub(crate) fn project_display_name(project: &Project) -> String {
    project
        .name
        .as_ref()
        .map(|name| name.trim())
        .filter(|name| !name.is_empty())
        .map(|name| name.to_string())
        .unwrap_or_else(|| UNTITLED_PROJECT_NAME.to_string())
}
