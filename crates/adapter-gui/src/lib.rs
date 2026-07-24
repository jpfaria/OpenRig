// Snapshot of complexity debt that existed on develop before the
// #548 build break was fixed (issue #576). Refactor of long fns and
// complex types is tracked under god-file ticket #276 and follow-ups.
// Allowing crate-wide keeps the QG honest about NEW regressions
// instead of perpetually re-reporting the existing snapshot.
#![allow(clippy::too_many_lines)]
#![allow(clippy::cognitive_complexity)]
#![allow(clippy::too_many_arguments)]
#![allow(clippy::type_complexity)]

mod audio_wizard_wiring;
mod back_to_launcher_wiring;
mod bank_scene_render;
mod bank_scene_session;
mod block_choose_type_callback;
mod block_delete_wiring;
mod block_drawer_close_wiring;
mod block_drawer_save_delete_wiring;
pub mod block_editor_param_tabs;
mod block_editor_window_lifecycle;
mod block_editor_window_delete;
mod block_editor_window_params;
mod block_editor_window_setup;
mod block_editor_window_wiring;
mod block_insert_callbacks;
mod block_model_search_wiring;
mod block_panel_dimensions;
mod block_parameter_wiring;
mod block_parameter_extras;
mod block_picker_wiring;
/// #614: compact chain view callbacks — also exposes public play/stop helpers
/// for integration tests (`compact_chain_di_loop_play`, `compact_chain_di_loop_stop`).
pub mod chain_binding_choices;
mod chain_block_crud_wiring;
mod chain_crud_wiring;
mod chain_editor_callbacks;
mod chain_editor_forwarders_wiring;
mod chain_editor_meta_io_callbacks;
mod chain_editor_save_cancel_callbacks;
mod chain_name_wiring;
mod chain_preset_wiring;
mod chain_rig_nav;
mod chain_rig_nav_wiring;
mod chain_row_wiring;
mod chain_row_wiring_actions;
mod chain_save_cancel_callbacks;
mod cli;
mod compact_block_layout;
mod compact_block_tabs;
mod compact_block_view;
mod compact_chain_block_handlers;
mod compact_chain_block_delete;
pub mod compact_chain_callbacks;
mod compact_chain_delete_wiring;
mod compact_chain_header_wiring;
mod compact_chain_param_handlers;
mod device_refresh_wiring;
mod device_settings_wiring;
/// #614: wires on_di_loop_choose_file (uses rfd — separate from chain_row_wiring
/// which is forbidden from importing rfd by issue #511).
mod di_loop_chooser_wiring;
/// #614: pure source-list builder + string→DiLoopSource mapper for the
/// chain-tile DI loop ComboBox (Task 7).
pub mod di_loop_ui_sources;
/// #614: DI loop wiring — apply_di_loop_event + di_loop_commands.
pub mod compact_chain_di_callbacks;
pub mod di_loop_wiring;
pub mod tone_doctor_compact_wiring;
pub mod tone_doctor_wiring;
/// #771: DI meter row values from the isolated playback's own peaks.
pub mod di_meter;
/// #771: pure option list + index mapping for the DI panel's output select.
pub mod di_output_options;
/// #771: window wiring for the DI panel's output select.
mod di_output_select_wiring;
/// #749: search-as-you-type filter for the chain DI loop source dropdown
/// (the shared `Select` component), mirroring the preset picker global.
pub mod di_source_picker_wiring;
mod insert_wiring;
mod plugin_info;
mod plugin_info_inline_wiring;
mod preset_save_wiring;
mod project_file_dialog_wiring;
mod project_settings_wiring;
mod recent_projects_wiring;
mod runtime_lifecycle;
mod runtime_sync_policy;
#[cfg(test)]
#[path = "runtime_sync_policy_tests.rs"]
mod runtime_sync_policy_tests;
mod select_chain_block_callback;
mod select_chain_callback;
mod selection_highlight;
pub(crate) mod settings;
/// #627: audio-device override mirror — keeps the shared in-memory `AppConfig`
/// in sync with a `SaveAudioSettings` disk write so that a subsequent
/// whole-config re-save does not clobber the user's buffer-size pick.
pub use settings::audio::apply_audio_override;
/// #607: pure path-override helpers (persist + mirror into the in-memory
/// `AppConfig`) exposed at the crate root for integration tests, without
/// widening the whole `settings` module to `pub`.
pub use settings::paths::{
    apply_evaluations_override, apply_plugins_override, apply_presets_override,
};
pub mod metronome_close;
mod metronome_controls_wiring;
mod metronome_events;
mod metronome_session;
mod metronome_wiring;
mod sample_rate;
pub mod spectrum_close;
mod spectrum_session;
mod spectrum_wiring;
mod thumbnails;
pub mod tuner_close;
mod tuner_session;
mod tuner_wiring;
pub mod ui_stall;
mod virtual_keyboard_wiring;
pub use bank_scene_render::{render as render_bank_scene, BankNavRow};
pub use bank_scene_session::{BankSceneEffect, BankSceneEvent, BankSceneState, InputNav};
pub(crate) use chain_editor_callbacks::setup_chain_editor_callbacks;
pub use cli::{
    parse_cli_args_from, parse_mcp_addr, parse_midi_map, resolve_mcp_addr, resolve_midi_map,
    validate_project_path, MidiMapArg,
};
pub(crate) use runtime_lifecycle::{
    assign_new_block_ids, remove_live_chain_runtime, stop_project_runtime, sync_block_toggle,
    sync_live_chain_runtime, sync_project_runtime, system_language, ui_index_to_real_block_index,
};
// #743: the live-sync planner is public so its decision (no device-IO resolve
// on a disable) is guarded by an integration test.
pub use runtime_lifecycle::{plan_live_sync, LiveSyncAction};

mod defaults;
pub(crate) use defaults::*;

mod audio_devices;
mod block_editor;
mod block_editor_param_items;
mod block_editor_persist;
mod block_editor_setters;
mod block_editor_values;
mod chain_editor;
mod default_io_binding;
mod eq;
pub mod graph_view_model;
mod helpers;
#[cfg(test)]
mod issue_692_project_open_time_tests;
#[cfg(test)]
mod issue_815_add_block_tabs_tests;
mod latency_probe;
/// #693: non-blocking logger init shared by binaries and tests.
pub mod logging;
mod meter_wiring;
mod meter_wiring_poll;
#[cfg(test)]
mod meter_wiring_row_update_tests;
mod midi_adapter_wiring;
pub mod midi_profile_wiring;
pub use midi_profile_wiring::start_midi_profiles;
pub mod mo_freshness;
mod model_search;
mod model_search_wiring;
mod preset_search;
mod project_ops;
mod project_ops_recents;
// #679: `pub` so the issue_599 integration test can reach
// `block_type_picker_items`. A private mod made `cargo test --tests` (and thus
// `cargo llvm-cov`) fail to compile, which silently zeroed all coverage.
pub mod project_view;
mod project_view_assets;
mod project_view_tooltips;
mod state;
mod ui_state;
slint::include_modules!();
#[cfg(test)]
pub(crate) use project_ops::{
    open_cli_project, project_display_name, project_title_for_path, register_recent_project,
};
use state::UNTITLED_PROJECT_NAME;
mod desktop_app;
mod desktop_app_block_models;
mod desktop_app_block_wiring;
mod desktop_app_chain_wiring;
mod desktop_app_cli_open;
mod desktop_app_init;
mod desktop_app_polling;
mod i18n;

pub use desktop_app::run_desktop_app;
pub use i18n::{apply_bundled_translation, init_translations, resolve_locale, SUPPORTED_LANGUAGES};

// Loads every YAML under crates/adapter-gui/locales/ at compile time.
// Keys MUST be semantic tags (e.g. `t!("error-no-project-loaded")`,
// never `t!("Nenhum projeto carregado.")`). Default fallback is en-US.
rust_i18n::i18n!("locales", fallback = "en-US");

#[cfg(test)]
#[path = "lib_tests.rs"]
mod tests;

#[cfg(test)]
#[path = "lib_recent_tests.rs"]
mod lib_recent_tests;

#[cfg(test)]
mod compact_block_search_wiring_tests;

#[cfg(test)]
mod chain_io_chip_label_tests;

#[cfg(test)]
mod project_view_stream_meters_tests;

#[cfg(test)]
mod touch_window_io_parity_tests;

// #716: Slint interaction tests — instantiate the real ProjectSettingsWindow
// headlessly and dispatch real pointer events, catching .slint structural bugs
// (TouchArea placement, focus recursion, callback wiring) that pure WireCtx
// tests cannot see.
#[cfg(test)]
#[path = "io_bindings_ui_tests.rs"]
mod io_bindings_ui_tests;
