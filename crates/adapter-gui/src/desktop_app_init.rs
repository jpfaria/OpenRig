//! Initial main-window state setup for `run_desktop_app`.
//!
//! Builds the Slint VecModels that the rest of the UI binds against
//! (devices, project rows, recent projects, chain device/channel pickers)
//! and seeds the AppWindow with its launch state — visible page, runtime
//! labels, audio settings wizard step, fullscreen flag, all the
//! `block_drawer_*` defaults so the drawer renders cleanly when first
//! opened. ProjectSettingsWindow gets its draft fields cleared too.
//!
//! Returns the VecModel handles the rest of `run_desktop_app` needs to
//! pass into wiring modules (so changes from a callback flow back through
//! the same `Rc` everywhere). No audio side effects — pure UI plumbing.

use std::cell::RefCell;
use std::rc::Rc;

use infra_cpal::AudioDeviceDescriptor;
use infra_filesystem::{AppConfig, GuiAudioSettings};
use slint::{ComponentHandle, ModelRc, SharedString, VecModel};
use ui_openrig::UiRuntimeContext;

use crate::audio_devices::build_project_device_rows;
use crate::project_ops::recent_project_items;
use crate::{
    AppWindow, ChannelOptionItem, DeviceSelectionItem, ProjectChainItem, ProjectSettingsWindow,
    RecentProjectItem,
};

pub(crate) struct InitialState {
    pub project_devices: Rc<VecModel<DeviceSelectionItem>>,
    pub project_chains: Rc<VecModel<ProjectChainItem>>,
    pub recent_projects: Rc<VecModel<RecentProjectItem>>,
    pub chain_input_device_options: Rc<VecModel<SharedString>>,
    pub chain_output_device_options: Rc<VecModel<SharedString>>,
    pub chain_input_channels: Rc<VecModel<ChannelOptionItem>>,
    pub chain_output_channels: Rc<VecModel<ChannelOptionItem>>,
}

#[allow(clippy::too_many_arguments)]
pub(crate) fn populate_initial_window_state(
    window: &AppWindow,
    project_settings_window: &ProjectSettingsWindow,
    context: &UiRuntimeContext,
    settings: &GuiAudioSettings,
    auto_save: bool,
    fullscreen: bool,
    needs_audio_settings: bool,
    input_chain_devices: &Rc<RefCell<Vec<AudioDeviceDescriptor>>>,
    output_chain_devices: &Rc<RefCell<Vec<AudioDeviceDescriptor>>>,
    app_config: &Rc<RefCell<AppConfig>>,
    input_devices: &Rc<VecModel<DeviceSelectionItem>>,
    output_devices: &Rc<VecModel<DeviceSelectionItem>>,
) -> InitialState {
    window.set_app_version(env!("CARGO_PKG_VERSION").into());
    window.set_show_project_launcher(true);
    window.set_show_project_setup(false);
    window.set_show_project_chains(false);
    window.set_show_chain_editor(false);
    window.set_show_project_settings(false);
    window.set_project_dirty(false);
    window.set_project_path_label("".into());
    window.set_project_title("Projeto".into());
    window.set_project_name_draft("".into());
    window.set_recent_project_search("".into());
    window.set_chain_editor_title("Nova chain".into());
    window.set_chain_editor_save_label("Criar chain".into());
    window.set_runtime_mode_label(context.runtime_mode.label().into());
    window.set_interaction_mode_label(context.interaction_mode.label().into());
    window.set_touch_optimized(context.capabilities.touch_optimized);
    window.set_auto_save(auto_save);
    window.set_fullscreen(fullscreen);
    if fullscreen {
        window.window().set_fullscreen(true);
    }
    window.set_show_audio_settings(needs_audio_settings);
    window.set_wizard_step(if settings.is_complete() { 1 } else { 0 });
    window.set_status_message("".into());

    let project_devices = Rc::new(VecModel::from(build_project_device_rows(
        &*input_chain_devices.borrow(),
        &*output_chain_devices.borrow(),
        &[],
    )));
    window.set_input_devices(ModelRc::from(input_devices.clone()));
    window.set_output_devices(ModelRc::from(output_devices.clone()));
    let project_chains = Rc::new(VecModel::from(Vec::<ProjectChainItem>::new()));
    window.set_project_chains(ModelRc::from(project_chains.clone()));
    let recent_projects = Rc::new(VecModel::from(recent_project_items(
        &app_config.borrow().recent_projects,
        "",
    )));
    window.set_recent_projects(ModelRc::from(recent_projects.clone()));

    let chain_input_device_options = Rc::new(VecModel::from(
        input_chain_devices
            .borrow()
            .iter()
            .map(|device| SharedString::from(device.name.clone()))
            .collect::<Vec<_>>(),
    ));
    let chain_output_device_options = Rc::new(VecModel::from(
        output_chain_devices
            .borrow()
            .iter()
            .map(|device| SharedString::from(device.name.clone()))
            .collect::<Vec<_>>(),
    ));
    let chain_input_channels = Rc::new(VecModel::from(Vec::<ChannelOptionItem>::new()));
    let chain_output_channels = Rc::new(VecModel::from(Vec::<ChannelOptionItem>::new()));
    window.set_chain_input_device_options(ModelRc::from(chain_input_device_options.clone()));
    window.set_chain_output_device_options(ModelRc::from(chain_output_device_options.clone()));
    window.set_chain_input_channels(ModelRc::from(chain_input_channels.clone()));
    window.set_chain_output_channels(ModelRc::from(chain_output_channels.clone()));
    window.set_selected_chain_input_device_index(-1);
    window.set_selected_chain_output_device_index(-1);
    window.set_selected_chain_block_chain_index(-1);
    window.set_selected_chain_block_index(-1);
    window.set_show_block_type_picker(false);
    window.set_show_block_model_picker(false);
    window.set_block_picker_title("".into());
    window.set_show_block_drawer(false);
    window.set_block_drawer_title("".into());
    window.set_block_drawer_confirm_label("Adicionar".into());
    window.set_block_drawer_status_message("".into());
    window.set_block_drawer_edit_mode(false);
    window.set_block_drawer_selected_type_index(-1);
    window.set_block_drawer_selected_model_index(-1);
    window.set_block_drawer_enabled(true);
    window.set_chain_draft_name("".into());
    project_settings_window.set_status_message("".into());
    project_settings_window.set_project_name_draft("".into());

    InitialState {
        project_devices,
        project_chains,
        recent_projects,
        chain_input_device_options,
        chain_output_device_options,
        chain_input_channels,
        chain_output_channels,
    }
}
