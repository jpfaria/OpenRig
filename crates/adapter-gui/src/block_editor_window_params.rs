//! Parameter-update callbacks on a per-block detached `BlockEditorWindow`.
//!
//! Seven near-identical handlers driving live parameter changes, plus the
//! VST3 native-editor opener:
//!
//! * `on_update_block_parameter_number` — knob; recomputes EQ curves and
//!   updates the curve editor in-place via `set_row_data` (avoids resetting
//!   TouchArea pressed state mid-drag).
//! * `on_update_block_parameter_number_text` — typed numeric value with `,`
//!   → `.` normalization.
//! * `on_update_block_parameter_bool` — toggle.
//! * `on_update_block_parameter_text` — string field.
//! * `on_select_block_parameter_option` — enum dropdown.
//! * `on_pick_block_parameter_file` — opens a file picker filtered by the
//!   parameter's allowed extensions.
//! * `on_open_vst3_editor` — opens the native VST3 plugin GUI.
//!
//! Each writes through to per-window models and schedules a debounced
//! persist via `schedule_block_editor_persist_for_block_win`. Wired once per
//! BlockEditorWindow from `block_editor_window_setup::create_and_wire`.

use std::cell::RefCell;
use std::rc::Rc;

use rfd::FileDialog;
use slint::{ComponentHandle, Model, SharedString, Timer, VecModel};

use infra_cpal::{AudioDeviceDescriptor, ProjectRuntimeController};

use crate::block_editor::{
    block_parameter_extensions, build_params_from_items,
    schedule_block_editor_persist_for_block_win, set_block_parameter_bool,
    set_block_parameter_number, set_block_parameter_option, set_block_parameter_text,
};
use crate::eq::compute_eq_curves;
use crate::state::{BlockEditorDraft, ProjectSession};
use crate::{
    AppWindow, BlockEditorWindow, BlockKnobOverlay, BlockParameterItem, CurveEditorPoint,
    ProjectChainItem,
};

pub(crate) struct BlockEditorWindowParamsCtx {
    pub win_draft: Rc<RefCell<Option<BlockEditorDraft>>>,
    pub win_param_items: Rc<VecModel<BlockParameterItem>>,
    pub win_knob_overlays: Rc<VecModel<BlockKnobOverlay>>,
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
    pub vst3_editor_handles: Rc<RefCell<Vec<Box<dyn project::vst3_editor::PluginEditorHandle>>>>,
    pub vst3_sample_rate: f64,
    pub auto_save: bool,
}

pub(crate) fn wire(
    win: &BlockEditorWindow,
    weak_main_window: slint::Weak<AppWindow>,
    ctx: BlockEditorWindowParamsCtx,
) {
    let BlockEditorWindowParamsCtx {
        win_draft,
        win_param_items,
        win_knob_overlays,
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
        vst3_editor_handles,
        vst3_sample_rate,
        auto_save,
    } = ctx;

    // on_update_block_parameter_number
    {
        let win_draft = win_draft.clone();
        let win_param_items = win_param_items.clone();
        let win_knob_overlays = win_knob_overlays.clone();
        let win_eq_band_curves = win_eq_band_curves.clone();
        let win_curve_editor_pts = win_curve_editor_pts.clone();
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
        win.on_update_block_parameter_number(move |path, value| {
            let Some(win) = weak_win.upgrade() else { return; };
            set_block_parameter_number(&win_param_items, path.as_str(), value);
            // Update overlay value so the knob indicator re-renders instantly
            for i in 0..win_knob_overlays.row_count() {
                if let Some(mut overlay) = win_knob_overlays.row_data(i) {
                    if overlay.path == path {
                        overlay.value = value;
                        win_knob_overlays.set_row_data(i, overlay);
                        break;
                    }
                }
            }
            // Recompute EQ curves and update curve editor point values in-place.
            // Use set_row_data instead of set_vec to avoid recreating elements (which
            // would reset the TouchArea pressed state and break drag interactions).
            if let Some(draft) = win_draft.borrow().as_ref() {
                let params = build_params_from_items(&win_param_items);
                let (eq_total, eq_bands) = compute_eq_curves(&draft.effect_type, &draft.model_id, &params);
                win_eq_band_curves.set_vec(eq_bands.into_iter().map(SharedString::from).collect::<Vec<_>>());
                win.set_eq_total_curve(eq_total.into());
                // Update matching curve editor point in-place by path
                let path_str = path.as_str();
                for idx in 0..win_curve_editor_pts.row_count() {
                    if let Some(mut pt) = win_curve_editor_pts.row_data(idx) {
                        if pt.y_path.as_str() == path_str {
                            pt.y_value = value;
                            pt.y_label = if value >= 0.0 {
                                format!("+{:.1}", value).into()
                            } else {
                                format!("{:.1}", value).into()
                            };
                            win_curve_editor_pts.set_row_data(idx, pt);
                            break;
                        } else if pt.has_x && pt.x_path.as_str() == path_str {
                            pt.x_value = value;
                            pt.x_label = if value >= 1000.0 {
                                format!("{:.1}k", value / 1000.0).into()
                            } else {
                                format!("{}Hz", value as i32).into()
                            };
                            win_curve_editor_pts.set_row_data(idx, pt);
                            break;
                        } else if pt.has_width && pt.width_path.as_str() == path_str {
                            pt.width_value = value;
                            win_curve_editor_pts.set_row_data(idx, pt);
                            break;
                        }
                    }
                }
            }
            if win_draft.borrow().as_ref().map(|d| d.block_index.is_some()).unwrap_or(false) {
                schedule_block_editor_persist_for_block_win(
                    &win_timer, weak_win.clone(), weak_main.clone(),
                    win_draft.clone(), win_param_items.clone(),
                    project_session.clone(), project_chains.clone(), project_runtime.clone(),
                    saved_project_snapshot.clone(), project_dirty.clone(),
                    input_chain_devices.clone(), output_chain_devices.clone(),
                    "block-window.number",
                    auto_save,
                );
            }
        });
    }

    // on_update_block_parameter_number_text
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
        let weak_main = weak_main_window.clone();
        let weak_win = win.as_weak();
        win.on_update_block_parameter_number_text(move |path, value_text| {
            let Some(_win) = weak_win.upgrade() else { return; };
            let normalized = value_text.replace(',', ".");
            let Ok(value) = normalized.parse::<f32>() else { return; };
            set_block_parameter_number(&win_param_items, path.as_str(), value);
            if win_draft.borrow().as_ref().map(|d| d.block_index.is_some()).unwrap_or(false) {
                schedule_block_editor_persist_for_block_win(
                    &win_timer, weak_win.clone(), weak_main.clone(),
                    win_draft.clone(), win_param_items.clone(),
                    project_session.clone(), project_chains.clone(), project_runtime.clone(),
                    saved_project_snapshot.clone(), project_dirty.clone(),
                    input_chain_devices.clone(), output_chain_devices.clone(),
                    "block-window.number-text",
                    auto_save,
                );
            }
        });
    }

    // on_update_block_parameter_bool
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
        let weak_main = weak_main_window.clone();
        let weak_win = win.as_weak();
        win.on_update_block_parameter_bool(move |path, value| {
            let Some(_win) = weak_win.upgrade() else { return; };
            set_block_parameter_bool(&win_param_items, path.as_str(), value);
            if win_draft.borrow().as_ref().map(|d| d.block_index.is_some()).unwrap_or(false) {
                schedule_block_editor_persist_for_block_win(
                    &win_timer, weak_win.clone(), weak_main.clone(),
                    win_draft.clone(), win_param_items.clone(),
                    project_session.clone(), project_chains.clone(), project_runtime.clone(),
                    saved_project_snapshot.clone(), project_dirty.clone(),
                    input_chain_devices.clone(), output_chain_devices.clone(),
                    "block-window.bool",
                    auto_save,
                );
            }
        });
    }

    // on_update_block_parameter_text
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
        let weak_main = weak_main_window.clone();
        let weak_win = win.as_weak();
        win.on_update_block_parameter_text(move |path, value| {
            let Some(_win) = weak_win.upgrade() else { return; };
            set_block_parameter_text(&win_param_items, path.as_str(), value.as_str());
            if win_draft.borrow().as_ref().map(|d| d.block_index.is_some()).unwrap_or(false) {
                schedule_block_editor_persist_for_block_win(
                    &win_timer, weak_win.clone(), weak_main.clone(),
                    win_draft.clone(), win_param_items.clone(),
                    project_session.clone(), project_chains.clone(), project_runtime.clone(),
                    saved_project_snapshot.clone(), project_dirty.clone(),
                    input_chain_devices.clone(), output_chain_devices.clone(),
                    "block-window.text",
                    auto_save,
                );
            }
        });
    }

    // on_select_block_parameter_option
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
        let weak_main = weak_main_window.clone();
        let weak_win = win.as_weak();
        win.on_select_block_parameter_option(move |path, index| {
            let Some(_win) = weak_win.upgrade() else { return; };
            set_block_parameter_option(&win_param_items, path.as_str(), index);
            if win_draft.borrow().as_ref().map(|d| d.block_index.is_some()).unwrap_or(false) {
                schedule_block_editor_persist_for_block_win(
                    &win_timer, weak_win.clone(), weak_main.clone(),
                    win_draft.clone(), win_param_items.clone(),
                    project_session.clone(), project_chains.clone(), project_runtime.clone(),
                    saved_project_snapshot.clone(), project_dirty.clone(),
                    input_chain_devices.clone(), output_chain_devices.clone(),
                    "block-window.option",
                    auto_save,
                );
            }
        });
    }

    // on_pick_block_parameter_file
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
        let weak_main = weak_main_window.clone();
        let weak_win = win.as_weak();
        win.on_pick_block_parameter_file(move |path| {
            let Some(_win) = weak_win.upgrade() else { return; };
            let extensions = block_parameter_extensions(&win_param_items, path.as_str());
            let mut dialog = FileDialog::new();
            if !extensions.is_empty() {
                let refs: Vec<&str> = extensions.iter().map(|v| v.as_str()).collect();
                dialog = dialog.add_filter("Arquivos suportados", &refs);
            }
            let Some(file) = dialog.pick_file() else { return; };
            set_block_parameter_text(&win_param_items, path.as_str(), file.to_string_lossy().as_ref());
            if win_draft.borrow().as_ref().map(|d| d.block_index.is_some()).unwrap_or(false) {
                schedule_block_editor_persist_for_block_win(
                    &win_timer, weak_win.clone(), weak_main.clone(),
                    win_draft.clone(), win_param_items.clone(),
                    project_session.clone(), project_chains.clone(), project_runtime.clone(),
                    saved_project_snapshot.clone(), project_dirty.clone(),
                    input_chain_devices.clone(), output_chain_devices.clone(),
                    "block-window.file",
                    auto_save,
                );
            }
        });
    }

    // on_open_vst3_editor (opens native plugin GUI window)
    {
        let vst3_handles = vst3_editor_handles.clone();
        let vst3_sr = vst3_sample_rate;
        win.on_open_vst3_editor(move |model_id| {
            match project::vst3_editor::open_vst3_editor(model_id.as_str(), vst3_sr) {
                Ok(handle) => { vst3_handles.borrow_mut().push(handle); }
                Err(e) => { log::error!("VST3 editor: failed '{}': {}", model_id, e); }
            }
        });
    }
}
