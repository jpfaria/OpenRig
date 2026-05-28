pub mod block_factory;
pub mod bridge;
pub mod chain_factory;
pub mod chain_validation;
pub mod command;
pub mod command_schema;
pub mod dispatcher;
pub mod event;
pub mod local_dispatcher;
mod local_dispatcher_block_edit;
mod local_dispatcher_block_lifecycle;
mod local_dispatcher_block_param;
mod local_dispatcher_chain_crud;
mod local_dispatcher_chain_io;
mod local_dispatcher_chain_order;
mod local_dispatcher_chain_save;
mod local_dispatcher_close;
mod local_dispatcher_diagnostic;
mod local_dispatcher_language;
mod local_dispatcher_output;
mod local_dispatcher_plugin_catalog;
mod local_dispatcher_preset;
mod local_dispatcher_project;
mod local_dispatcher_recent;
mod local_dispatcher_recent_register;
mod local_dispatcher_rig;
mod local_dispatcher_selection;
pub mod preset_file;
pub mod project_save;
pub mod publishing_dispatcher;
pub mod query;
pub mod render_handler;
pub mod selection_state;
pub mod session;

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
