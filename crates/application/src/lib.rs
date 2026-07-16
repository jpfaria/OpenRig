// Snapshot of complexity debt that existed on develop before the
// #548 build break was fixed (issue #576). Refactor of long fns and
// complex types is tracked under god-file ticket #276 and follow-ups.
// Allowing crate-wide keeps the QG honest about NEW regressions
// instead of perpetually re-reporting the existing snapshot.
#![allow(clippy::too_many_lines)]
#![allow(clippy::cognitive_complexity)]
#![allow(clippy::too_many_arguments)]
#![allow(clippy::type_complexity)]

pub mod app_config_persist;
pub mod block_factory;
pub mod bridge;
pub mod chain_factory;
pub mod command;
pub mod command_schema;
pub mod di_loader;
pub mod dispatcher;
pub mod event;
pub mod local_dispatcher;
mod local_dispatcher_access;
mod local_dispatcher_attach;
mod local_dispatcher_block_edit;
mod local_dispatcher_block_lifecycle;
mod local_dispatcher_block_param;
mod local_dispatcher_chain_crud;
mod local_dispatcher_chain_io;
mod local_dispatcher_chain_order;
mod local_dispatcher_chain_save;
mod local_dispatcher_close;
mod local_dispatcher_di_loop;
mod local_dispatcher_diagnostic;
mod local_dispatcher_io_binding;
mod local_dispatcher_ir_reseed;
mod local_dispatcher_language;
mod local_dispatcher_midi_system;
mod local_dispatcher_output;
mod local_dispatcher_paths;
mod local_dispatcher_queries;
mod local_dispatcher_plugin_catalog;
mod local_dispatcher_preset;
mod local_dispatcher_project;
mod local_dispatcher_recent;
mod local_dispatcher_recent_register;
mod local_dispatcher_rig;
mod local_dispatcher_selection;
mod local_dispatcher_subsystems;
/// #693: command side-effect writes run on a dedicated worker thread —
/// `flush()` is the durability barrier for shutdown and round-trips.
pub mod persist_worker;
pub mod preset_file;
pub mod project_save;
pub mod publishing_dispatcher;
pub mod query;
pub mod render_handler;
pub mod selection_state;
pub mod session;
/// #693: published immutable state snapshot — transports serve reads
/// concurrently on their own thread (API-style), never via the GUI tick.
pub mod snapshot;

pub use selection_state::SelectionState;
pub mod validate;

#[cfg(test)]
#[path = "local_dispatcher_tests.rs"]
mod local_dispatcher_tests;

#[cfg(test)]
#[path = "local_dispatcher_midi_block_nav_tests.rs"]
mod local_dispatcher_midi_block_nav_tests;

#[cfg(test)]
#[path = "local_dispatcher_midi_e2e_tests.rs"]
mod local_dispatcher_midi_e2e_tests;

#[cfg(test)]
#[path = "local_dispatcher_paths_tests.rs"]
mod local_dispatcher_paths_tests;

#[cfg(test)]
#[path = "local_dispatcher_rig_tests.rs"]
mod local_dispatcher_rig_tests;
