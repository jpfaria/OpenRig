//! Lifecycle callbacks on a per-block detached `BlockEditorWindow`.
//!
//! Seven handlers covering the window's full lifecycle outside per-parameter
//! edits:
//!
//! * `on_choose_block_model` — model swap inside the editor; rebuilds knob
//!   overlays / EQ curves / multi-slider / curve-editor data and schedules a
//!   debounced persist when editing an existing block.
//! * `on_toggle_block_drawer_enabled` — enable/disable toggle for the block,
//!   syncs the live runtime and the project dirty marker.
//! * `on_save_block_drawer` — persist + close (also clears the selected
//!   block on the main window).
//! * `on_delete_block_drawer` — confirm dialog, remove from chain, resync
//!   runtime, close.
//! * `on_show_plugin_info` — opens a `PluginInfoWindow` with description /
//!   license / homepage / screenshot for the current model.
//! * `on_close_block_drawer` — discard and close.
//! * `window().on_close_requested` — also drops the entry from
//!   `open_block_windows` so reopening the same chain/block creates a fresh
//!   window (stale window cleanup).
//!
//! Wired once per BlockEditorWindow from `block_editor_window_setup::create_and_wire`.

use std::cell::RefCell;
use std::rc::Rc;

use slint::{ComponentHandle, SharedString, Timer, VecModel};

use infra_cpal::{AudioDeviceDescriptor, ProjectRuntimeController};
use project::catalog::{model_brand, model_display_name, model_type_label};

use application::command::Command;
use application::dispatcher::CommandDispatcher;
use application::event::Event;

use crate::block_editor::{
    block_parameter_items_for_model, build_knob_overlays, build_params_from_items,
    persist_block_editor_draft, schedule_block_editor_persist_for_block_win,
};
use crate::eq::{
    build_curve_editor_points, build_multi_slider_points, compute_eq_curves, eq_viz_sample_rate,
};
use crate::helpers::show_child_window;
use crate::plugin_info;
use crate::project_ops::sync_project_dirty;
use crate::project_view::{
    block_model_picker_items, load_screenshot_image, replace_project_chains, set_selected_block,
};
use crate::runtime_lifecycle::{sync_block_toggle, sync_live_chain_runtime, system_language};
use crate::state::{BlockEditorDraft, BlockWindow, ProjectSession, SelectedBlock};
use crate::{
    AppWindow, BlockEditorWindow, BlockKnobOverlay, BlockParameterItem, CurveEditorPoint,
    MultiSliderPoint, PluginInfoWindow, ProjectChainItem,
};

/// Compute the knob-grid window dimensions in Rust and push them into
/// the Slint `BlockEditorWindow` via its `in` properties. Must be
/// called whenever the knob source changes (initial editor setup OR
/// model switch inside the editor). The pure policy lives in
/// `block_panel_dimensions` and is gated by unit tests — Slint never
/// re-derives the wrap math. Issue #500.
pub(crate) fn apply_panel_dimensions(win: &BlockEditorWindow) {
    use slint::Model;
    let overlay_count = win.get_block_knob_overlays().row_count();
    let param_count = win.get_block_parameter_items().row_count();
    // Slint hides the param grid when overlays are present
    // (`block-knob-overlays.length == 0` gates `params-visible`), so
    // the count that drives layout is whichever source actually renders.
    let knob_count = if overlay_count > 0 {
        overlay_count
    } else {
        param_count
    };
    let has_eq_widget = win.get_multi_slider_points().row_count() > 0
        || win.get_curve_editor_points().row_count() > 0;
    let type_idx = win.get_block_drawer_selected_type_index();
    let types = win.get_block_type_options();
    let use_panel_editor = if type_idx >= 0 {
        types
            .row_data(type_idx as usize)
            .map(|t| t.use_panel_editor)
            .unwrap_or(false)
    } else {
        // No selection yet — default to the panel editor so the window
        // sizes for the upcoming knob grid instead of the form-editor
        // fallback (which would briefly flash a 520x820 window).
        true
    };

    let dims = crate::block_panel_dimensions::compute(crate::block_panel_dimensions::PanelInputs {
        knob_count,
        use_panel_editor,
        has_eq_widget,
    });
    win.set_panel_knob_window_width(dims.window_width_px);
    win.set_panel_knob_window_height(dims.window_height_px);
    win.set_panel_knob_inner_height(dims.inner_panel_height_px);
    win.set_panel_grid_cols(dims.grid_cols as i32);
    win.set_panel_grid_rows(dims.grid_rows as i32);

    // Resize the OS window so the new dimensions take effect immediately
    // — Linux WMs ignore Slint min/max/preferred-* without this (#479).
    let pw = win.get_panel_width();
    let ph = win.get_panel_height();
    if pw.is_finite() && ph.is_finite() && pw > 0.0 && ph > 0.0 {
        win.window()
            .set_size(slint::WindowSize::Logical(slint::LogicalSize::new(pw, ph)));
    }
}

pub(crate) struct BlockEditorWindowLifecycleCtx {
    pub win_draft: Rc<RefCell<Option<BlockEditorDraft>>>,
    pub win_param_items: Rc<VecModel<BlockParameterItem>>,
    pub win_knob_overlays: Rc<VecModel<BlockKnobOverlay>>,
    pub win_multi_slider_pts: Rc<VecModel<MultiSliderPoint>>,
    pub win_curve_editor_pts: Rc<VecModel<CurveEditorPoint>>,
    pub win_eq_band_curves: Rc<VecModel<SharedString>>,
    pub win_timer: Rc<Timer>,
    pub project_session: Rc<RefCell<Option<ProjectSession>>>,
    pub project_chains: Rc<VecModel<ProjectChainItem>>,
    pub project_runtime: Rc<RefCell<Option<ProjectRuntimeController>>>,
    pub saved_project_snapshot: Rc<RefCell<Option<String>>>,
    pub project_dirty: Rc<RefCell<bool>>,
    pub input_chain_devices: Rc<RefCell<Vec<AudioDeviceDescriptor>>>,
    pub output_chain_devices: Rc<RefCell<Vec<AudioDeviceDescriptor>>>,
    pub selected_block: Rc<RefCell<Option<SelectedBlock>>>,
    pub open_block_windows: Rc<RefCell<Vec<BlockWindow>>>,
    pub plugin_info_window: Rc<RefCell<Option<PluginInfoWindow>>>,
    pub chain_index: usize,
    pub block_index: usize,
    pub auto_save: bool,
}

pub(crate) fn wire(
    win: &BlockEditorWindow,
    weak_main_window: slint::Weak<AppWindow>,
    ctx: BlockEditorWindowLifecycleCtx,
) {
    let BlockEditorWindowLifecycleCtx {
        win_draft,
        win_param_items,
        win_knob_overlays,
        win_multi_slider_pts,
        win_curve_editor_pts,
        win_eq_band_curves,
        win_timer,
        project_session,
        project_chains,
        project_runtime,
        saved_project_snapshot,
        project_dirty,
        input_chain_devices,
        output_chain_devices,
        selected_block,
        open_block_windows,
        plugin_info_window,
        chain_index,
        block_index,
        auto_save,
    } = ctx;

    // on_choose_block_model
    {
        let win_draft = win_draft.clone();
        let win_param_items = win_param_items.clone();
        let win_knob_overlays = win_knob_overlays.clone();
        let win_multi_slider_pts = win_multi_slider_pts.clone();
        let win_curve_editor_pts = win_curve_editor_pts.clone();
        let win_eq_band_curves = win_eq_band_curves.clone();
        let win_timer = win_timer.clone();
        let project_session = project_session.clone();
        let project_chains = project_chains.clone();
        let project_runtime = project_runtime.clone();
        let saved_project_snapshot = saved_project_snapshot.clone();
        let project_dirty = project_dirty.clone();
        let input_chain_devices = input_chain_devices.clone();
        let output_chain_devices = output_chain_devices.clone();
        let weak_main = weak_main_window.clone();
        let weak_win = win.as_weak();
        win.on_choose_block_model(move |index| {
            let Some(win) = weak_win.upgrade() else {
                return;
            };
            let mut draft_borrow = win_draft.borrow_mut();
            let Some(draft) = draft_borrow.as_mut() else {
                return;
            };
            let models = block_model_picker_items(&draft.effect_type, &draft.instrument);
            let Some(model) = models.get(index as usize) else {
                return;
            };
            draft.model_id = model.model_id.to_string();
            draft.effect_type = model.effect_type.to_string();
            // Seed the new model's knobs from the manifest (output_db #655,
            // noise_gate #675) via the single source in `block_factory`.
            let seeded = application::block_factory::default_params_for_model(
                &model.effect_type,
                &model.model_id,
            )
            .unwrap_or_default();
            let new_params =
                block_parameter_items_for_model(&model.effect_type, &model.model_id, &seeded);
            let overlays = build_knob_overlays(
                project::catalog::model_knob_layout(&model.effect_type, &model.model_id),
                &new_params,
            );
            win_knob_overlays.set_vec(overlays);
            win_param_items.set_vec(new_params);
            // Update EQ widgets for the new model
            let default_params = build_params_from_items(&win_param_items);
            win_multi_slider_pts.set_vec(build_multi_slider_points(
                &model.effect_type,
                &model.model_id,
                &default_params,
            ));
            win_curve_editor_pts.set_vec(build_curve_editor_points(
                &model.effect_type,
                &model.model_id,
                &default_params,
            ));
            let (eq_total, eq_bands) = compute_eq_curves(
                &model.effect_type,
                &model.model_id,
                &default_params,
                eq_viz_sample_rate(&project_runtime),
            );
            win_eq_band_curves.set_vec(
                eq_bands
                    .into_iter()
                    .map(SharedString::from)
                    .collect::<Vec<_>>(),
            );
            win.set_eq_total_curve(eq_total.into());
            // Re-size the window for the new knob count / EQ state
            // (issue #500: model switch inside the editor must resize).
            apply_panel_dimensions(&win);
            drop(draft_borrow);
            if win_draft
                .borrow()
                .as_ref()
                .map(|d| d.block_index.is_some())
                .unwrap_or(false)
            {
                schedule_block_editor_persist_for_block_win(
                    &win_timer,
                    weak_win.clone(),
                    weak_main.clone(),
                    win_draft.clone(),
                    win_param_items.clone(),
                    project_session.clone(),
                    project_chains.clone(),
                    project_runtime.clone(),
                    saved_project_snapshot.clone(),
                    project_dirty.clone(),
                    input_chain_devices.clone(),
                    output_chain_devices.clone(),
                    "block-window.choose-model",
                    auto_save,
                );
            }
        });
    }

    // on_toggle_block_drawer_enabled
    {
        let win_draft = win_draft.clone();
        let project_session = project_session.clone();
        let project_chains = project_chains.clone();
        let project_runtime = project_runtime.clone();
        let saved_project_snapshot = saved_project_snapshot.clone();
        let project_dirty = project_dirty.clone();
        let input_chain_devices = input_chain_devices.clone();
        let output_chain_devices = output_chain_devices.clone();
        let weak_main = weak_main_window.clone();
        let weak_win = win.as_weak();
        win.on_toggle_block_drawer_enabled(move || {
            let Some(win) = weak_win.upgrade() else {
                return;
            };
            let Some(main) = weak_main.upgrade() else {
                return;
            };
            // Step 1: read chain_id and block_id from the draft + project (immutable).
            let (chain_id, block_id) = {
                let (chain_index, block_index) = {
                    let draft_borrow = win_draft.borrow();
                    let Some(draft) = draft_borrow.as_ref() else {
                        return;
                    };
                    let Some(bi) = draft.block_index else {
                        return;
                    };
                    (draft.chain_index, bi)
                };
                let session_borrow = project_session.borrow();
                let Some(session) = session_borrow.as_ref() else {
                    return;
                };
                let proj = session.project.borrow();
                let Some(chain) = proj.chains.get(chain_index) else {
                    return;
                };
                let Some(block) = chain.blocks.get(block_index) else {
                    return;
                };
                (chain.id.clone(), block.id.clone())
            };
            // Step 2: dispatch Command::ToggleBlockEnabled — mutates project via shared Rc.
            let new_enabled = {
                let session_borrow = project_session.borrow();
                let Some(session) = session_borrow.as_ref() else {
                    return;
                };
                match session.dispatcher.dispatch(Command::ToggleBlockEnabled {
                    chain: chain_id.clone(),
                    block: block_id.clone(),
                }) {
                    Ok(events) => events.into_iter().find_map(|e| {
                        if let Event::BlockEnabledChanged { enabled, .. } = e {
                            Some(enabled)
                        } else {
                            None
                        }
                    }),
                    Err(e) => {
                        log::error!("[adapter-gui] block-window.toggle-enabled dispatch: {e}");
                        main.set_block_drawer_status_message(e.to_string().into());
                        return;
                    }
                }
            };
            let Some(new_enabled) = new_enabled else {
                log::error!(
                    "[adapter-gui] block-window.toggle-enabled: no BlockEnabledChanged event"
                );
                return;
            };
            // Step 3: update draft enabled state.
            if let Some(draft) = win_draft.borrow_mut().as_mut() {
                draft.enabled = new_enabled;
            }
            // Step 4: resync runtime + refresh UI.
            let mut session_borrow = project_session.borrow_mut();
            let Some(session) = session_borrow.as_mut() else {
                return;
            };
            if let Err(e) =
                sync_block_toggle(&project_runtime, session, &chain_id, &block_id, new_enabled)
            {
                log::error!("[adapter-gui] block-window.toggle-enabled runtime sync: {e}");
                main.set_block_drawer_status_message(e.to_string().into());
                return;
            }
            replace_project_chains(
                &project_chains,
                &session.project.borrow(),
                &input_chain_devices.borrow(),
                &output_chain_devices.borrow(),
            &[]
            );
            sync_project_dirty(
                &main,
                session,
                &saved_project_snapshot,
                &project_dirty,
                auto_save,
            );
            drop(session_borrow);
            win.set_block_drawer_enabled(new_enabled);
        });
    }

    // on_save_block_drawer (edit mode - saves and closes)
    {
        let win_draft = win_draft.clone();
        let win_param_items = win_param_items.clone();
        let win_timer = win_timer.clone();
        let project_session = project_session.clone();
        let project_chains = project_chains.clone();
        let project_runtime = project_runtime.clone();
        let saved_project_snapshot = saved_project_snapshot.clone();
        let project_dirty = project_dirty.clone();
        let input_chain_devices = input_chain_devices.clone();
        let output_chain_devices = output_chain_devices.clone();
        let selected_block_save = selected_block.clone();
        let open_block_windows_save = open_block_windows.clone();
        let weak_main = weak_main_window.clone();
        let weak_win = win.as_weak();
        win.on_save_block_drawer(move || {
            let Some(win) = weak_win.upgrade() else {
                return;
            };
            let Some(main) = weak_main.upgrade() else {
                return;
            };
            win_timer.stop();
            let Some(draft) = win_draft.borrow().clone() else {
                return;
            };
            if let Err(e) = persist_block_editor_draft(
                &main,
                &draft,
                &win_param_items,
                &project_session,
                &project_chains,
                &project_runtime,
                &saved_project_snapshot,
                &project_dirty,
                &input_chain_devices.borrow(),
                &output_chain_devices.borrow(),
                true,
                auto_save,
            ) {
                log::error!("[adapter-gui] block-window.save: {e}");
                main.set_block_drawer_status_message(e.to_string().into());
                return;
            }
            *selected_block_save.borrow_mut() = None;
            set_selected_block(&main, None, None);
            open_block_windows_save.borrow_mut().retain(|bw| {
                bw.chain_index != draft.chain_index
                    || bw.block_index != draft.block_index.unwrap_or(usize::MAX)
            });
            let _ = win.hide();
        });
    }

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
            &[]
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

    // Touch unused models so the helper above (which mutates them) doesn't
    // emit dead-code warnings on partial wiring branches.
    let _ = (
        &win_knob_overlays,
        &win_multi_slider_pts,
        &win_curve_editor_pts,
        &win_eq_band_curves,
    );
}
