//! Wiring for the main window's chain create/edit callbacks.
//!
//! Owns `on_add_chain` (creates a fresh `ChainEditorWindow`, builds a default
//! draft from the project + connected devices, populates I/O groups + channel
//! pickers + instrument selector, opens the editor inline-fullscreen or as a
//! child window) and `on_configure_chain` (same but populates the editor from
//! an existing chain via `chain_draft_from_chain`).
//!
//! Both callbacks delegate to `setup_chain_editor_callbacks` (still in lib.rs
//! pending its own slice) for the editor-window callback registration.

use std::cell::RefCell;
use std::rc::Rc;

use slint::{ComponentHandle, SharedString, Timer, VecModel};

use infra_cpal::{AudioDeviceDescriptor, ProjectRuntimeController};

use crate::audio_devices::{
    build_input_channel_items, build_output_channel_items, ensure_devices_loaded,
    replace_channel_options, selected_device_index,
};
use crate::chain_editor::{
    apply_chain_editor_labels, chain_draft_from_chain, create_chain_draft,
    instrument_string_to_index,
};
use crate::helpers::{clear_status, set_status_error, show_child_window};
use crate::io_groups::apply_chain_io_groups;
use crate::setup_chain_editor_callbacks;
use crate::state::{ChainDraft, IoBlockInsertDraft, ProjectSession};
use crate::{
    AppWindow, ChainEditorWindow, ChainInputWindow, ChainOutputWindow, ChannelOptionItem,
    ProjectChainItem,
};

pub(crate) struct ChainCrudCtx {
    pub project_session: Rc<RefCell<Option<ProjectSession>>>,
    pub chain_draft: Rc<RefCell<Option<ChainDraft>>>,
    pub input_chain_devices: Rc<RefCell<Vec<AudioDeviceDescriptor>>>,
    pub output_chain_devices: Rc<RefCell<Vec<AudioDeviceDescriptor>>>,
    pub chain_input_channels: Rc<VecModel<ChannelOptionItem>>,
    pub chain_output_channels: Rc<VecModel<ChannelOptionItem>>,
    pub chain_editor_window: Rc<RefCell<Option<ChainEditorWindow>>>,
    pub chain_input_device_options: Rc<VecModel<SharedString>>,
    pub chain_output_device_options: Rc<VecModel<SharedString>>,
    pub project_chains: Rc<VecModel<ProjectChainItem>>,
    pub project_runtime: Rc<RefCell<Option<ProjectRuntimeController>>>,
    pub saved_project_snapshot: Rc<RefCell<Option<String>>>,
    pub project_dirty: Rc<RefCell<bool>>,
    pub io_block_insert_draft: Rc<RefCell<Option<IoBlockInsertDraft>>>,
    pub toast_timer: Rc<Timer>,
    pub auto_save: bool,
    pub fullscreen: bool,
}

pub(crate) fn wire(
    window: &AppWindow,
    chain_input_window: &ChainInputWindow,
    chain_output_window: &ChainOutputWindow,
    ctx: ChainCrudCtx,
) {
    let ChainCrudCtx {
        project_session,
        chain_draft,
        input_chain_devices,
        output_chain_devices,
        chain_input_channels,
        chain_output_channels,
        chain_editor_window,
        chain_input_device_options,
        chain_output_device_options,
        project_chains,
        project_runtime,
        saved_project_snapshot,
        project_dirty,
        io_block_insert_draft,
        toast_timer,
        auto_save,
        fullscreen,
    } = ctx;

    {
        let weak_window = window.as_weak();
        let project_session = project_session.clone();
        let chain_draft = chain_draft.clone();
        let input_chain_devices = input_chain_devices.clone();
        let output_chain_devices = output_chain_devices.clone();
        let chain_input_channels = chain_input_channels.clone();
        let chain_output_channels = chain_output_channels.clone();
        let chain_editor_window = chain_editor_window.clone();
        let chain_input_device_options = chain_input_device_options.clone();
        let chain_output_device_options = chain_output_device_options.clone();
        let project_chains = project_chains.clone();
        let project_runtime = project_runtime.clone();
        let saved_project_snapshot = saved_project_snapshot.clone();
        let project_dirty = project_dirty.clone();
        let weak_input_window = chain_input_window.as_weak();
        let weak_output_window = chain_output_window.as_weak();
        let io_block_insert_draft = io_block_insert_draft.clone();
        let toast_timer = toast_timer.clone();
        window.on_add_chain(move || {
            log::info!("on_add_chain triggered");
            let Some(window) = weak_window.upgrade() else {
                return;
            };
            ensure_devices_loaded(&input_chain_devices, &output_chain_devices);
            let editor_window = match ChainEditorWindow::new() {
                Ok(w) => w,
                Err(e) => {
                    log::error!("Failed to create chain editor window: {e}");
                    return;
                }
            };
            {
                use slint::Global;
                crate::Locale::get(&editor_window).set_font_family(
                    crate::i18n::font_for_persisted_runtime().into(),
                );
            }
            setup_chain_editor_callbacks(
                &editor_window,
                window.as_weak(),
                chain_draft.clone(),
                project_session.clone(),
                project_chains.clone(),
                project_runtime.clone(),
                saved_project_snapshot.clone(),
                project_dirty.clone(),
                input_chain_devices.clone(),
                output_chain_devices.clone(),
                chain_input_device_options.clone(),
                chain_output_device_options.clone(),
                chain_input_channels.clone(),
                chain_output_channels.clone(),
                weak_input_window.clone(),
                weak_output_window.clone(),
                io_block_insert_draft.clone(),
                toast_timer.clone(),
                auto_save,
            );
            *chain_editor_window.borrow_mut() = Some(editor_window);
            let ce_borrow = chain_editor_window.borrow();
            let editor_window = ce_borrow.as_ref().unwrap();
            let borrow = project_session.borrow();
            let Some(session) = borrow.as_ref() else {
                set_status_error(&window, &toast_timer, &rust_i18n::t!("error-no-project-loaded"));
                return;
            };
            let devs_in = input_chain_devices.borrow();
            let devs_out = output_chain_devices.borrow();
            let draft = create_chain_draft(&session.project, &devs_in, &devs_out);
            *chain_draft.borrow_mut() = Some(draft.clone());
            apply_chain_editor_labels(&window, &draft);
            apply_chain_io_groups(&window, editor_window, &draft, &devs_in, &devs_out);
            if let Some(input_group) = draft.inputs.first() {
                replace_channel_options(
                    &chain_input_channels,
                    build_input_channel_items(input_group, &devs_in),
                );
            }
            if let Some(output_group) = draft.outputs.first() {
                replace_channel_options(
                    &chain_output_channels,
                    build_output_channel_items(output_group, &devs_out),
                );
            }
            window.set_chain_draft_name(draft.name.clone().into());
            editor_window.set_chain_name(draft.name.clone().into());
            editor_window.set_editor_title(window.get_chain_editor_title());
            editor_window.set_editor_save_label(window.get_chain_editor_save_label());
            editor_window.set_is_create_mode(true);
            editor_window
                .set_selected_instrument_index(instrument_string_to_index(&draft.instrument));
            window.set_selected_chain_input_device_index(selected_device_index(
                &devs_in,
                draft.inputs.first().and_then(|i| i.device_id.as_deref()),
            ));
            window.set_selected_chain_output_device_index(selected_device_index(
                &devs_out,
                draft.outputs.first().and_then(|o| o.device_id.as_deref()),
            ));
            editor_window.set_status_message("".into());
            clear_status(&window, &toast_timer);
            window.set_show_chain_editor(true);
            if fullscreen {
                window.set_chain_editor_input_groups(editor_window.get_input_groups());
                window.set_chain_editor_output_groups(editor_window.get_output_groups());
                window.set_chain_editor_is_create_mode(editor_window.get_is_create_mode());
                window.set_chain_editor_selected_instrument_index(
                    editor_window.get_selected_instrument_index(),
                );
            } else {
                show_child_window(window.window(), editor_window.window());
            }
        });
    }
    {
        let weak_window = window.as_weak();
        let project_session = project_session.clone();
        let chain_draft = chain_draft.clone();
        let input_chain_devices = input_chain_devices.clone();
        let output_chain_devices = output_chain_devices.clone();
        let chain_input_channels = chain_input_channels.clone();
        let chain_output_channels = chain_output_channels.clone();
        let chain_editor_window = chain_editor_window.clone();
        let chain_input_device_options = chain_input_device_options.clone();
        let chain_output_device_options = chain_output_device_options.clone();
        let project_chains = project_chains.clone();
        let project_runtime = project_runtime.clone();
        let saved_project_snapshot = saved_project_snapshot.clone();
        let project_dirty = project_dirty.clone();
        let weak_input_window = chain_input_window.as_weak();
        let weak_output_window = chain_output_window.as_weak();
        let io_block_insert_draft = io_block_insert_draft.clone();
        let toast_timer = toast_timer.clone();
        window.on_configure_chain(move |index| {
            let Some(window) = weak_window.upgrade() else {
                return;
            };
            ensure_devices_loaded(&input_chain_devices, &output_chain_devices);
            let editor_window = match ChainEditorWindow::new() {
                Ok(w) => w,
                Err(e) => {
                    log::error!("Failed to create chain editor window: {e}");
                    return;
                }
            };
            {
                use slint::Global;
                crate::Locale::get(&editor_window).set_font_family(
                    crate::i18n::font_for_persisted_runtime().into(),
                );
            }
            setup_chain_editor_callbacks(
                &editor_window,
                window.as_weak(),
                chain_draft.clone(),
                project_session.clone(),
                project_chains.clone(),
                project_runtime.clone(),
                saved_project_snapshot.clone(),
                project_dirty.clone(),
                input_chain_devices.clone(),
                output_chain_devices.clone(),
                chain_input_device_options.clone(),
                chain_output_device_options.clone(),
                chain_input_channels.clone(),
                chain_output_channels.clone(),
                weak_input_window.clone(),
                weak_output_window.clone(),
                io_block_insert_draft.clone(),
                toast_timer.clone(),
                auto_save,
            );
            *chain_editor_window.borrow_mut() = Some(editor_window);
            let ce_borrow = chain_editor_window.borrow();
            let editor_window = ce_borrow.as_ref().unwrap();
            let session_borrow = project_session.borrow();
            let Some(session) = session_borrow.as_ref() else {
                set_status_error(&window, &toast_timer, &rust_i18n::t!("error-no-project-loaded"));
                return;
            };
            let Some(chain) = session.project.chains.get(index as usize) else {
                set_status_error(&window, &toast_timer, &rust_i18n::t!("error-invalid-chain"));
                return;
            };
            let devs_in = input_chain_devices.borrow();
            let devs_out = output_chain_devices.borrow();
            let draft = chain_draft_from_chain(index as usize, chain);
            if let Some(input_group) = draft.inputs.first() {
                replace_channel_options(
                    &chain_input_channels,
                    build_input_channel_items(input_group, &devs_in),
                );
            }
            if let Some(output_group) = draft.outputs.first() {
                replace_channel_options(
                    &chain_output_channels,
                    build_output_channel_items(output_group, &devs_out),
                );
            }
            window.set_chain_draft_name(draft.name.clone().into());
            editor_window.set_chain_name(draft.name.clone().into());
            window.set_selected_chain_input_device_index(selected_device_index(
                &devs_in,
                draft.inputs.first().and_then(|i| i.device_id.as_deref()),
            ));
            window.set_selected_chain_output_device_index(selected_device_index(
                &devs_out,
                draft.outputs.first().and_then(|o| o.device_id.as_deref()),
            ));
            *chain_draft.borrow_mut() = Some(draft);
            if let Some(draft) = chain_draft.borrow().as_ref() {
                apply_chain_editor_labels(&window, draft);
                apply_chain_io_groups(&window, editor_window, draft, &devs_in, &devs_out);
                editor_window.set_editor_title(window.get_chain_editor_title());
                editor_window.set_editor_save_label(window.get_chain_editor_save_label());
                editor_window.set_is_create_mode(false);
                editor_window
                    .set_selected_instrument_index(instrument_string_to_index(&draft.instrument));
            }
            editor_window.set_status_message("".into());
            clear_status(&window, &toast_timer);
            window.set_show_chain_editor(true);
            if fullscreen {
                window.set_chain_editor_input_groups(editor_window.get_input_groups());
                window.set_chain_editor_output_groups(editor_window.get_output_groups());
                window.set_chain_editor_is_create_mode(editor_window.get_is_create_mode());
                window.set_chain_editor_selected_instrument_index(
                    editor_window.get_selected_instrument_index(),
                );
            } else {
                show_child_window(window.window(), editor_window.window());
            }
        });
    }
}
