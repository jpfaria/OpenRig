mod thumbnails;
mod plugin_info;
mod spectrum_session;
mod spectrum_wiring;
mod tuner_session;
mod tuner_wiring;
mod insert_wiring;
mod device_settings_wiring;
mod chain_io_picker_wiring;
mod block_editor_window_wiring;
mod recent_projects_wiring;
mod project_file_dialog_wiring;
mod device_refresh_wiring;
mod audio_wizard_wiring;
mod project_settings_wiring;
mod back_to_launcher_wiring;
mod chain_preset_wiring;
mod audio_settings_save_wiring;
mod chain_crud_wiring;
mod chain_io_main_wiring;
mod chain_input_groups_wiring;
mod chain_output_groups_wiring;
mod chain_row_wiring;
mod chain_editor_forwarders_wiring;
mod chain_block_crud_wiring;
mod virtual_keyboard_wiring;
mod chain_name_wiring;
mod chain_editor_callbacks;
mod chain_editor_meta_io_callbacks;
mod chain_editor_save_cancel_callbacks;
mod chain_editor_input_endpoint_callbacks;
mod chain_editor_output_endpoint_callbacks;
mod chain_io_save_wiring;
mod block_model_search_wiring;
mod block_picker_wiring;
mod block_drawer_close_wiring;
mod block_delete_wiring;
mod vst3_editor_wiring;
mod block_drawer_save_delete_wiring;
mod block_parameter_wiring;
mod compact_chain_callbacks;
mod compact_chain_block_handlers;
mod compact_chain_param_handlers;
mod block_insert_callbacks;
mod block_choose_type_callback;
mod chain_io_fullscreen_callbacks;
mod runtime_lifecycle;
mod select_chain_block_callback;
mod block_editor_window_setup;
mod block_editor_window_params;
mod block_editor_window_lifecycle;
mod cli;
mod chain_save_cancel_callbacks;
pub use cli::parse_cli_args_from;
pub(crate) use chain_editor_callbacks::setup_chain_editor_callbacks;
pub(crate) use runtime_lifecycle::{
    assign_new_block_ids, remove_live_chain_runtime, stop_project_runtime,
    sync_live_chain_runtime, sync_project_runtime, system_language,
    ui_index_to_real_block_index,
};
mod defaults;
pub(crate) use defaults::*;
mod audio_devices;
mod block_editor;
mod block_editor_param_items;
mod block_editor_persist;
mod block_editor_setters;
mod block_editor_values;
mod chain_editor;
mod eq;
mod helpers;
mod io_groups;
mod latency_probe;
mod project_ops;
mod project_view;
mod project_view_assets;
mod project_view_tooltips;
mod state;
mod model_search;
mod model_search_wiring;
mod ui_state;
mod visual_config;
slint::include_modules!();
use state::UNTITLED_PROJECT_NAME;
#[cfg(test)]
pub(crate) use project_ops::{
    open_cli_project, project_display_name, project_title_for_path, register_recent_project,
};
mod desktop_app;
mod desktop_app_polling;
mod desktop_app_cli_open;
mod desktop_app_block_models;
mod desktop_app_init;
mod desktop_app_block_wiring;
mod desktop_app_chain_wiring;
pub use desktop_app::run_desktop_app;
#[cfg(test)]
#[path = "lib_tests.rs"]
mod tests;
