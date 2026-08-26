//! Responsibility: keeps the historical `project_ops_recents` path pointing at the three files it became.
//!
//! It held three: the recent-projects list, project path resolution, and the
//! display name (#873).

pub(crate) use crate::project_display_name::project_display_name;
pub(crate) use crate::project_path::{canonical_project_path, parse_path_argument};
pub(crate) use crate::recent_projects::{
    mark_recent_project_invalid, recent_project_items, register_recent_project,
    sync_recent_projects,
};
