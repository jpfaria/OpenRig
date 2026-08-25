//! Responsibility: routes the read-only project introspection surface.

//! Read-only project introspection for adapters. The project uses opaque
//! string IDs (`chain:<uuid>`, `chain:<uuid>:block:<uuid>`), not ordinals,
//! so a `midi-map.yaml` author (or the MCP `openrig://ids` resource) needs a
//! way to discover them. This is the single place that formats that listing;
//! adapters never re-walk `Project` themselves.

pub use crate::query_block_params::get_block_params;
pub use crate::query_ids::list_ids;
pub use crate::query_paths::resolved_paths_json;
pub use crate::query_plugins::{find_plugins, get_plugin, get_plugin_params, list_plugin_catalog};
pub use crate::query_presets::{list_chain_presets, list_project_presets};

#[cfg(test)]
#[path = "query_chain_presets_tests.rs"]
mod chain_presets_tests;

#[cfg(test)]
#[path = "query_project_presets_tests.rs"]
mod project_presets_tests;

#[cfg(test)]
#[path = "query_plugin_params_tests.rs"]
mod plugin_params_tests;

// `query_tests.rs` hangs off this module and reaches its fixtures through
// `super::*`, as it did before the split (#873).
#[cfg(test)]
pub(crate) use project::project::Project;

#[cfg(test)]
#[path = "query_tests.rs"]
mod tests;
