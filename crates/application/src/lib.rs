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
mod local_dispatcher_preset;
mod local_dispatcher_project;
mod local_dispatcher_recent;
mod local_dispatcher_rig;
pub mod publishing_dispatcher;
pub mod session;
pub mod validate;

#[cfg(test)]
#[path = "local_dispatcher_tests.rs"]
mod local_dispatcher_tests;

#[cfg(test)]
#[path = "local_dispatcher_rig_tests.rs"]
mod local_dispatcher_rig_tests;
