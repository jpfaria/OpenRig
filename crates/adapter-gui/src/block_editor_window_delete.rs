//! Block-editor drawer delete + plugin-info/close callback wiring
//! (issue #792 split from block_editor_window_lifecycle.rs).

use slint::ComponentHandle;

use application::command::Command;
use application::dispatcher::CommandDispatcher;
use project::catalog::{model_brand, model_display_name, model_type_label};

use crate::helpers::show_child_window;
use crate::plugin_info;
use crate::project_ops::sync_project_dirty;
use crate::project_view::{load_screenshot_image, replace_project_chains, set_selected_block};
use crate::runtime_lifecycle::{sync_live_chain_runtime, system_language};
use crate::{AppWindow, BlockEditorWindow, PluginInfoWindow};

use crate::block_editor_window_lifecycle::BlockEditorWindowLifecycleCtx;

pub(crate) fn wire_block_delete(
    win: &BlockEditorWindow,
    weak_main_window: &slint::Weak<AppWindow>,
    ctx: &BlockEditorWindowLifecycleCtx,
) {
    let win_draft = &ctx.win_draft;
    let win_timer = &ctx.win_timer;
    let project_session = &ctx.project_session;
    let project_chains = &ctx.project_chains;
    let project_runtime = &ctx.project_runtime;
    let saved_project_snapshot = &ctx.saved_project_snapshot;
    let project_dirty = &ctx.project_dirty;
    let input_chain_devices = &ctx.input_chain_devices;
    let output_chain_devices = &ctx.output_chain_devices;
    let selected_block = &ctx.selected_block;
    let open_block_windows = &ctx.open_block_windows;
    let auto_save = ctx.auto_save;


    // on_delete_block_drawer (trash icon) — opens the in-window overlay.
    // Issue #360: the actual delete moved to on_confirm_delete_block below;
    // the previous native-dialog path is gone (native popup did not suit
    // Orange Pi touch sessions and stole focus on macOS).
    {
        let win_draft = win_draft.clone();
        let win_timer = win_timer.clone();
        let weak_win = win.as_weak();
        win.on_delete_block_drawer(move || {
            let Some(win) = weak_win.upgrade() else {
                return;
            };
            win_timer.stop();
            let Some(draft) = win_draft.borrow().clone() else {
                return;
            };
            if draft.block_index.is_none() {
                return;
            }
            win.set_confirm_delete_block_name(draft.model_id.into());
            win.set_show_confirm_delete_block(true);
        });
    }


    // on_cancel_delete_block — just hide the overlay.
    {
        let weak_win = win.as_weak();
        win.on_cancel_delete_block(move || {
            if let Some(win) = weak_win.upgrade() {
                win.set_show_confirm_delete_block(false);
            }
        });
    }


    // on_confirm_delete_block — execute the deletion the overlay just gated.
    {
        let win_draft = win_draft.clone();
        let project_session = project_session.clone();
        let project_chains = project_chains.clone();
        let project_runtime = project_runtime.clone();
        let saved_project_snapshot = saved_project_snapshot.clone();
        let project_dirty = project_dirty.clone();
        let input_chain_devices = input_chain_devices.clone();
        let output_chain_devices = output_chain_devices.clone();
        let selected_block_delete = selected_block.clone();
        let open_block_windows_delete = open_block_windows.clone();
        let weak_main = weak_main_window.clone();
        let weak_win = win.as_weak();
        win.on_confirm_delete_block(move || {
            let Some(win) = weak_win.upgrade() else {
                return;
            };
            // Hide overlay first so any error toast renders on the
            // window, not behind the modal backdrop.
            win.set_show_confirm_delete_block(false);
            let Some(main) = weak_main.upgrade() else {
                return;
            };
            let Some(draft) = win_draft.borrow().clone() else {
                return;
            };
            let Some(block_index) = draft.block_index else {
                return;
            };
            let mut session_borrow = project_session.borrow_mut();
            let Some(session) = session_borrow.as_mut() else {
                return;
            };
            // Resolve chain_id and block_id before dispatching.
            let (chain_id, block_id) = {
                let proj = session.project.borrow();
                let Some(chain) = proj.chains.get(draft.chain_index) else {
                    return;
                };
                let Some(block) = chain.blocks.get(block_index) else {
                    return;
                };
                (chain.id.clone(), block.id.clone())
            };
            // Dispatch Command::RemoveBlock — mutates project via shared Rc.
            if let Err(e) = session.dispatcher.dispatch(Command::RemoveBlock {
                chain: chain_id.clone(),
                block: block_id,
            }) {
                log::error!("[adapter-gui] block-window.delete dispatch: {e}");
                if let Some(w) = weak_main.upgrade() {
                    w.set_block_drawer_status_message(e.to_string().into());
                }
                return;
            }
            if let Err(e) = sync_live_chain_runtime(&project_runtime, session, &chain_id) {
                log::error!("[adapter-gui] block-window.delete: {e}");
                if let Some(w) = weak_main.upgrade() {
                    w.set_block_drawer_status_message(e.to_string().into());
                }
                return;
            }
            replace_project_chains(
                &project_chains,
                &session.project.borrow(),
                &input_chain_devices.borrow(),
                &output_chain_devices.borrow(),
                &[],
            );
            sync_project_dirty(
                &main,
                session,
                &saved_project_snapshot,
                &project_dirty,
                auto_save,
            );
            drop(session_borrow);
            *selected_block_delete.borrow_mut() = None;
            set_selected_block(&main, None, None);
            open_block_windows_delete
                .borrow_mut()
                .retain(|bw| bw.chain_index != draft.chain_index || bw.block_index != block_index);
            let _ = win.hide();
        });
    }
}

pub(crate) fn wire_plugin_info_close(
    win: &BlockEditorWindow,
    weak_main_window: &slint::Weak<AppWindow>,
    ctx: &BlockEditorWindowLifecycleCtx,
) {
    let win_draft = &ctx.win_draft;
    let selected_block = &ctx.selected_block;
    let open_block_windows = &ctx.open_block_windows;
    let plugin_info_window = &ctx.plugin_info_window;
    let chain_index = ctx.chain_index;
    let block_index = ctx.block_index;


    // on_show_plugin_info
    {
        let weak_main = weak_main_window.clone();
        let plugin_info_window = plugin_info_window.clone();
        win.on_show_plugin_info(move |effect_type, model_id| {
            let Some(window) = weak_main.upgrade() else {
                return;
            };
            let effect_type = effect_type.to_string();
            let model_id = model_id.to_string();

            let display_name = model_display_name(&effect_type, &model_id);
            let brand = model_brand(&effect_type, &model_id);
            let type_label = model_type_label(&effect_type, &model_id);

            let lang = system_language();
            let meta = plugin_info::plugin_metadata(&lang, &model_id);

            let (screenshot_img, has_screenshot) = load_screenshot_image(&effect_type, &model_id);

            let info_win = match PluginInfoWindow::new() {
                Ok(w) => w,
                Err(e) => {
                    log::error!("Failed to create PluginInfoWindow: {}", e);
                    return;
                }
            };
            {
                use slint::Global;
                crate::Locale::get(&info_win)
                    .set_font_family(crate::i18n::font_for_persisted_runtime().into());
            }

            info_win.set_plugin_name(display_name.into());
            info_win.set_brand(brand.into());
            info_win.set_type_label(type_label.into());
            info_win.set_description(meta.description.into());
            info_win.set_license(meta.license.into());
            info_win.set_has_homepage(!meta.homepage.is_empty());
            info_win.set_homepage(meta.homepage.clone().into());
            info_win.set_screenshot(screenshot_img);
            info_win.set_has_screenshot(has_screenshot);

            {
                let homepage = meta.homepage.clone();
                info_win.on_open_homepage(move || {
                    plugin_info::open_homepage(&homepage);
                });
            }

            {
                let win_weak = info_win.as_weak();
                info_win.on_close_window(move || {
                    if let Some(w) = win_weak.upgrade() {
                        let _ = w.window().hide();
                    }
                });
            }

            *plugin_info_window.borrow_mut() = Some(info_win);
            if let Some(w) = plugin_info_window.borrow().as_ref() {
                show_child_window(window.window(), w.window());
            }
        });
    }


    // on_close_block_drawer (close without saving)
    {
        let win_draft = win_draft.clone();
        let open_block_windows_close = open_block_windows.clone();
        let selected_block_close = selected_block.clone();
        let weak_main = weak_main_window.clone();
        let weak_win = win.as_weak();
        win.on_close_block_drawer(move || {
            let Some(win) = weak_win.upgrade() else {
                return;
            };
            let Some(main) = weak_main.upgrade() else {
                return;
            };
            let draft_borrow = win_draft.borrow();
            if let Some(draft) = draft_borrow.as_ref() {
                open_block_windows_close.borrow_mut().retain(|bw| {
                    bw.chain_index != draft.chain_index || Some(bw.block_index) != draft.block_index
                });
            }
            drop(draft_borrow);
            *selected_block_close.borrow_mut() = None;
            set_selected_block(&main, None, None);
            let _ = win.hide();
        });
    }


    // Clean up stream timer when block editor is closed via the window X button.
    {
        let open_block_windows_close = open_block_windows.clone();
        let ci = chain_index;
        let bi = block_index;
        win.window().on_close_requested(move || {
            open_block_windows_close
                .borrow_mut()
                .retain(|bw| bw.chain_index != ci || bw.block_index != bi);
            slint::CloseRequestResponse::HideWindow
        });
    }
}

