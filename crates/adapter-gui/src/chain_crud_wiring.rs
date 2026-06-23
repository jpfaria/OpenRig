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

use slint::{ComponentHandle, Timer, VecModel};

use infra_cpal::{AudioDeviceDescriptor, ProjectRuntimeController};
use infra_filesystem::AppConfig;

use crate::audio_devices::ensure_devices_loaded;
use crate::chain_editor::{
    apply_chain_editor_labels, chain_draft_from_chain, create_chain_draft,
    instrument_string_to_index,
};
use crate::helpers::{clear_status, set_status_error, show_child_window};
use crate::setup_chain_editor_callbacks;
use crate::state::{ChainDraft, ProjectSession};
use crate::{AppWindow, ChainEditorWindow, ProjectChainItem};

pub(crate) struct ChainCrudCtx {
    pub project_session: Rc<RefCell<Option<ProjectSession>>>,
    pub chain_draft: Rc<RefCell<Option<ChainDraft>>>,
    pub input_chain_devices: Rc<RefCell<Vec<AudioDeviceDescriptor>>>,
    pub output_chain_devices: Rc<RefCell<Vec<AudioDeviceDescriptor>>>,
    pub chain_editor_window: Rc<RefCell<Option<ChainEditorWindow>>>,
    pub project_chains: Rc<VecModel<ProjectChainItem>>,
    pub project_runtime: Rc<RefCell<Option<ProjectRuntimeController>>>,
    pub saved_project_snapshot: Rc<RefCell<Option<String>>>,
    pub project_dirty: Rc<RefCell<bool>>,
    pub toast_timer: Rc<Timer>,
    pub app_config: Rc<RefCell<AppConfig>>,
    pub auto_save: bool,
    pub fullscreen: bool,
}

pub(crate) fn wire(window: &AppWindow, ctx: ChainCrudCtx) {
    let ChainCrudCtx {
        project_session,
        chain_draft,
        input_chain_devices,
        output_chain_devices,
        chain_editor_window,
        project_chains,
        project_runtime,
        saved_project_snapshot,
        project_dirty,
        toast_timer,
        app_config,
        auto_save,
        fullscreen,
    } = ctx;

    {
        let weak_window = window.as_weak();
        let project_session = project_session.clone();
        let chain_draft = chain_draft.clone();
        let input_chain_devices = input_chain_devices.clone();
        let output_chain_devices = output_chain_devices.clone();
        let chain_editor_window = chain_editor_window.clone();
        let project_chains = project_chains.clone();
        let project_runtime = project_runtime.clone();
        let saved_project_snapshot = saved_project_snapshot.clone();
        let project_dirty = project_dirty.clone();
        let toast_timer = toast_timer.clone();
        let app_config = app_config.clone();
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
                crate::Locale::get(&editor_window)
                    .set_font_family(crate::i18n::font_for_persisted_runtime().into());
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
                toast_timer.clone(),
                auto_save,
            );
            *chain_editor_window.borrow_mut() = Some(editor_window);
            let ce_borrow = chain_editor_window.borrow();
            let editor_window = ce_borrow.as_ref().unwrap();
            let borrow = project_session.borrow();
            let Some(session) = borrow.as_ref() else {
                set_status_error(
                    &window,
                    &toast_timer,
                    &rust_i18n::t!("error-no-project-loaded"),
                );
                return;
            };
            let devs_in = input_chain_devices.borrow();
            let devs_out = output_chain_devices.borrow();
            let draft = create_chain_draft(&*session.project.borrow(), &devs_in, &devs_out);
            *chain_draft.borrow_mut() = Some(draft.clone());
            apply_chain_editor_labels(&window, &draft);
            // #716: feed the binding checklist (none selected for a new chain).
            editor_window.set_bindings(slint::ModelRc::new(VecModel::from(
                crate::chain_binding_choices::binding_choices(
                    &app_config.borrow().io_bindings,
                    &draft.io_binding_ids,
                ),
            )));
            window.set_chain_draft_name(draft.name.clone().into());
            editor_window.set_chain_name(draft.name.clone().into());
            editor_window.set_editor_title(window.get_chain_editor_title());
            editor_window.set_editor_save_label(window.get_chain_editor_save_label());
            editor_window.set_is_create_mode(true);
            editor_window
                .set_selected_instrument_index(instrument_string_to_index(&draft.instrument));
            editor_window.set_status_message("".into());
            clear_status(&window, &toast_timer);
            window.set_show_chain_editor(true);
            if fullscreen {
                window.set_chain_editor_bindings(editor_window.get_bindings());
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
        let chain_editor_window = chain_editor_window.clone();
        let project_chains = project_chains.clone();
        let project_runtime = project_runtime.clone();
        let saved_project_snapshot = saved_project_snapshot.clone();
        let project_dirty = project_dirty.clone();
        let toast_timer = toast_timer.clone();
        let app_config = app_config.clone();
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
                crate::Locale::get(&editor_window)
                    .set_font_family(crate::i18n::font_for_persisted_runtime().into());
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
                toast_timer.clone(),
                auto_save,
            );
            *chain_editor_window.borrow_mut() = Some(editor_window);
            let ce_borrow = chain_editor_window.borrow();
            let editor_window = ce_borrow.as_ref().unwrap();
            let draft = {
                let session_borrow = project_session.borrow();
                let Some(session) = session_borrow.as_ref() else {
                    set_status_error(
                        &window,
                        &toast_timer,
                        &rust_i18n::t!("error-no-project-loaded"),
                    );
                    return;
                };
                let proj = session.project.borrow();
                let Some(chain) = proj.chains.get(index as usize) else {
                    set_status_error(&window, &toast_timer, &rust_i18n::t!("error-invalid-chain"));
                    return;
                };
                chain_draft_from_chain(index as usize, chain)
            };
            window.set_chain_draft_name(draft.name.clone().into());
            editor_window.set_chain_name(draft.name.clone().into());
            *chain_draft.borrow_mut() = Some(draft);
            if let Some(draft) = chain_draft.borrow().as_ref() {
                apply_chain_editor_labels(&window, draft);
                // #716: feed the binding checklist, pre-checking the chain's
                // currently-selected bindings.
                editor_window.set_bindings(slint::ModelRc::new(VecModel::from(
                    crate::chain_binding_choices::binding_choices(
                        &app_config.borrow().io_bindings,
                        &draft.io_binding_ids,
                    ),
                )));
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
                window.set_chain_editor_bindings(editor_window.get_bindings());
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
