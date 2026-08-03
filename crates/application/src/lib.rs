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
pub mod audio_taps;
pub mod block_factory;
pub mod bridge;
pub mod chain_factory;
pub mod command;
pub mod command_schema;
pub mod di_loader;
pub mod dispatcher;
pub mod event;
mod event_scope;
#[cfg(test)]
#[path = "issue_85_port_position_tests.rs"]
mod issue_85_port_position_tests;
pub mod live_source;
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
mod local_dispatcher_looper;
mod local_dispatcher_metronome;
mod local_dispatcher_midi_system;
mod local_dispatcher_output;
mod local_dispatcher_parity_829;
mod local_dispatcher_paths;
mod local_dispatcher_plugin_catalog;
mod local_dispatcher_preset;
mod local_dispatcher_project;
mod local_dispatcher_queries;
mod local_dispatcher_recent;
mod local_dispatcher_recent_register;
mod local_dispatcher_rig;
mod local_dispatcher_runtime_sync;
mod local_dispatcher_selection;
mod local_dispatcher_subsystems;
mod local_dispatcher_tone_doctor;
mod local_dispatcher_trait;
pub mod looper_audio;
/// #127: the metronome's control-plane state — settings, chosen output and
/// tap history — owned by the dispatcher so every transport shares one truth.
pub mod metronome_state;
/// #693: command side-effect writes run on a dedicated worker thread —
/// `flush()` is the durability barrier for shutdown and round-trips.
pub mod persist_worker;
pub mod preset_file;
pub mod project_save;
pub mod publishing_dispatcher;
pub mod query;
pub mod query_analyzers;
pub mod query_chain_quality;
pub mod query_di;
pub mod query_latency;
pub mod query_loopers;
/// #831: the single `QueryKind` resolver every transport answers through —
/// one match, one payload shape, one error string.
pub mod read;
pub mod render_handler;
pub mod runtime_control;
pub mod selection_state;
pub mod session;
/// #693: published immutable state snapshot — transports serve reads
/// concurrently on their own thread (API-style), never via the GUI tick.
pub mod snapshot;
/// #791: the Tone Doctor's verdict as transport-agnostic data + the commands
/// that apply its measured fix.
pub mod tone_doctor_report;

pub use selection_state::SelectionState;
pub mod validate;

#[cfg(test)]
#[path = "local_dispatcher_tests.rs"]
mod local_dispatcher_tests;

#[cfg(test)]
#[path = "ld_block2_tests.rs"]
mod ld_block2;

#[cfg(test)]
#[path = "ld_block_param_tests.rs"]
mod ld_block_param;

#[cfg(test)]
#[path = "ld_chain_tests.rs"]
mod ld_chain;

#[cfg(test)]
#[path = "ld_savechain_tests.rs"]
mod ld_savechain;

#[cfg(test)]
#[path = "ld_insert_tests.rs"]
mod ld_insert;

#[cfg(test)]
#[path = "ld_project_tests.rs"]
mod ld_project;

#[cfg(test)]
#[path = "ld_preset_tests.rs"]
mod ld_preset;

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
#[path = "local_dispatcher_parity_829_tests.rs"]
mod local_dispatcher_parity_829_tests;

#[cfg(test)]
#[path = "local_dispatcher_rig_tests.rs"]
mod local_dispatcher_rig_tests;
