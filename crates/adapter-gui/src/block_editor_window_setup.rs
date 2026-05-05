//! Builds a fresh per-block detached `BlockEditorWindow` and wires its
//! callbacks.
//!
//! When the user clicks an effect block in the chain (and inline editing
//! isn't enabled), we open a separate window so the editor stays out of the
//! way. This module owns the construction of that window:
//!
//! 1. Creates the `BlockEditorWindow` instance.
//! 2. Builds independent per-window models (`Rc<VecModel<...>>`) so changes
//!    here don't reach back into the main `AppWindow`'s shared models.
//! 3. Wires the standalone search popup (delegated to
//!    `model_search_wiring::wire_standalone_block_editor_window`).
//! 4. Sets initial window state (title, type/model indices, EQ curves,
//!    knob overlays, multi-slider, curve editor).
//! 5. Starts a 50 ms stream-polling timer for utility blocks (kept alive
//!    across enable toggles so streaming starts/stops with the block).
//! 6. Delegates parameter handlers to `block_editor_window_params::wire`
//!    and lifecycle handlers to `block_editor_window_lifecycle::wire`.
//!
//! Returns `(window, Option<Rc<Timer>>)` so the caller can push them onto
//! the `open_block_windows` registry.

use std::cell::RefCell;
use std::rc::Rc;

use slint::{ComponentHandle, ModelRc, SharedString, Timer, VecModel};

use infra_cpal::{AudioDeviceDescriptor, ProjectRuntimeController};
use project::param::ParameterSet;

use crate::block_editor::{block_parameter_items_for_editor, build_knob_overlays};
use crate::eq::{build_curve_editor_points, build_multi_slider_points, compute_eq_curves};
use crate::project_view::{
    block_model_index_from_items, block_model_picker_items, block_model_picker_labels,
    block_type_index, block_type_picker_items,
};
use crate::state::{BlockEditorData, BlockEditorDraft, BlockWindow, ProjectSession, SelectedBlock};
use crate::{
    block_editor_window_lifecycle, block_editor_window_params, AppWindow, BlockEditorWindow,
    BlockStreamData, BlockStreamEntry, PluginInfoWindow, ProjectChainItem,
};

pub(crate) struct BlockEditorWindowSetupCtx {
    pub chain_index: usize,
    pub block_index: usize,
    pub instrument: String,
    pub effect_type: String,
    pub model_id: String,
    pub enabled: bool,
    pub editor_data: BlockEditorData,
    pub block_id: domain::ids::BlockId,
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
    pub vst3_editor_handles: Rc<RefCell<Vec<Box<dyn project::vst3_editor::PluginEditorHandle>>>>,
    pub vst3_sample_rate: f64,
    pub auto_save: bool,
}

pub(crate) fn create_and_wire(
    weak_main_window: slint::Weak<AppWindow>,
    ctx: BlockEditorWindowSetupCtx,
) -> Result<(BlockEditorWindow, Option<Rc<Timer>>), slint::PlatformError> {
    let BlockEditorWindowSetupCtx {
        chain_index,
        block_index,
        instrument,
        effect_type,
        model_id,
        enabled,
        editor_data,
        block_id,
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
        vst3_editor_handles,
        vst3_sample_rate,
        auto_save,
    } = ctx;

    let win = BlockEditorWindow::new()?;
    // Per-window models (independent copies of the data)
    let win_model_options = Rc::new(VecModel::from(block_model_picker_items(
        &effect_type,
        &instrument,
    )));
    // Filtered list starts as a copy of the full list so the
    // popup shows everything when first opened. The search
    // callback below replaces it on every keystroke.
    let win_filtered_model_options = Rc::new(VecModel::from(block_model_picker_items(
        &effect_type,
        &instrument,
    )));
    let win_model_labels = Rc::new(VecModel::from(block_model_picker_labels(
        &block_model_picker_items(&effect_type, &instrument),
    )));
    let win_param_items_vec = block_parameter_items_for_editor(&editor_data);
    let win_knob_overlays = Rc::new(VecModel::from(build_knob_overlays(
        project::catalog::model_knob_layout(&effect_type, &model_id),
        &win_param_items_vec,
    )));
    let win_param_items = Rc::new(VecModel::from(win_param_items_vec));
    let win_draft = Rc::new(RefCell::new(Some(BlockEditorDraft {
        chain_index,
        block_index: Some(block_index),
        before_index: block_index,
        instrument: instrument.clone(),
        effect_type: effect_type.clone(),
        model_id: model_id.clone(),
        enabled,
        is_select: editor_data.is_select,
    })));
    let win_timer = Rc::new(Timer::default());

    // Populate window — ALL data set independently (no sync from AppWindow)
    let type_index = block_type_index(&effect_type, &instrument);
    let model_index = block_model_index_from_items(&win_model_options, &model_id);
    win.set_block_type_options(ModelRc::from(Rc::new(VecModel::from(
        block_type_picker_items(&instrument),
    ))));
    win.set_block_model_options(ModelRc::from(win_model_options.clone()));
    win.set_filtered_block_model_options(ModelRc::from(win_filtered_model_options.clone()));
    win.set_block_model_option_labels(ModelRc::from(win_model_labels.clone()));
    crate::model_search_wiring::wire_standalone_block_editor_window(
        &win,
        win_model_options.clone(),
        win_filtered_model_options.clone(),
    );
    win.set_block_parameter_items(ModelRc::from(win_param_items.clone()));
    win.set_block_knob_overlays(ModelRc::from(win_knob_overlays.clone()));
    let win_multi_slider_pts = Rc::new(VecModel::from(build_multi_slider_points(
        &effect_type,
        &model_id,
        &editor_data.params,
    )));
    let win_curve_editor_pts = Rc::new(VecModel::from(build_curve_editor_points(
        &effect_type,
        &model_id,
        &editor_data.params,
    )));
    let (win_eq_total, win_eq_bands) =
        compute_eq_curves(&effect_type, &model_id, &editor_data.params);
    let win_eq_band_curves = Rc::new(VecModel::from(
        win_eq_bands
            .into_iter()
            .map(SharedString::from)
            .collect::<Vec<_>>(),
    ));
    win.set_multi_slider_points(ModelRc::from(win_multi_slider_pts.clone()));
    win.set_curve_editor_points(ModelRc::from(win_curve_editor_pts.clone()));
    win.set_eq_total_curve(win_eq_total.into());
    win.set_eq_band_curves(ModelRc::from(win_eq_band_curves.clone()));
    win.set_block_drawer_selected_type_index(type_index);
    win.set_block_drawer_selected_model_index(model_index);
    win.set_block_drawer_edit_mode(true);
    win.set_block_drawer_enabled(enabled);
    win.set_block_drawer_status_message("".into());
    // Set window title
    let title_label = {
        use slint::Model;
        win_model_options
            .row_data(model_index as usize)
            .map(|m| m.label.to_string())
            .unwrap_or_else(|| "Block".to_string())
    };
    win.set_block_window_title(format!("OpenRig · {}", title_label).into());

    // Stream data timer — polls stream data when block produces it (e.g. tuner).
    // Start the timer regardless of current enabled state: when the user enables
    // the block while the popup is open, stream data must appear without reopening.
    let mut block_stream_timer: Option<Rc<Timer>> = None;
    let is_utility = effect_type == block_core::EFFECT_TYPE_UTILITY;
    log::info!(
        "[block-editor-stream] block='{}' effect_type='{}' model='{}' enabled={} is_utility={}",
        block_id.0,
        effect_type,
        model_id,
        enabled,
        is_utility
    );
    if is_utility {
        log::info!(
            "[block-editor-stream] starting stream timer for block '{}'",
            block_id.0
        );
        let stream_timer = Rc::new(Timer::default());
        let weak_win_stream = win.as_weak();
        let project_runtime_stream = project_runtime.clone();
        let block_id_for_stream = block_id.clone();
        let mut poll_count: u32 = 0;
        stream_timer.start(
            slint::TimerMode::Repeated,
            std::time::Duration::from_millis(50),
            move || {
                let Some(win) = weak_win_stream.upgrade() else { return; };
                let runtime_borrow = project_runtime_stream.borrow();
                // No utility block currently produces a "spectrum" stream
                // (the spectrum_analyzer block was promoted to a top-bar
                // feature in #320). Kept generic for future stream blocks.
                let kind: slint::SharedString = "stream".into();
                let Some(runtime) = runtime_borrow.as_ref() else {
                    poll_count += 1;
                    if poll_count % 40 == 0 {
                        log::debug!("[block-editor-stream] runtime not available (poll #{})", poll_count);
                    }
                    return;
                };
                if let Some(entries) = runtime.poll_stream(&block_id_for_stream) {
                    let slint_entries: Vec<BlockStreamEntry> = entries.iter().map(|e| BlockStreamEntry {
                        key: e.key.clone().into(),
                        value: e.value,
                        text: e.text.clone().into(),
                        peak: e.peak,
                    }).collect();
                    poll_count += 1;
                    if poll_count % 40 == 1 {
                        log::debug!("[block-editor-stream] poll #{}: {} entries, first={:?}", poll_count, slint_entries.len(), entries.first().map(|e| &e.key));
                    }
                    win.set_block_stream_data(BlockStreamData {
                        active: true,
                        stream_kind: kind,
                        entries: ModelRc::from(Rc::new(VecModel::from(slint_entries))),
                    });
                } else {
                    poll_count += 1;
                    if poll_count % 40 == 0 {
                        log::debug!("[block-editor-stream] poll #{}: no entries (silence or no runtime handle)", poll_count);
                    }
                    win.set_block_stream_data(BlockStreamData {
                        active: false,
                        stream_kind: kind.clone(),
                        entries: ModelRc::default(),
                    });
                }
            },
        );
        block_stream_timer = Some(stream_timer);
    }

    let _ = (&editor_data, &type_index); // editor_data fully consumed above

    block_editor_window_params::wire(
        &win,
        weak_main_window.clone(),
        block_editor_window_params::BlockEditorWindowParamsCtx {
            win_draft: win_draft.clone(),
            win_param_items: win_param_items.clone(),
            win_knob_overlays: win_knob_overlays.clone(),
            win_curve_editor_pts: win_curve_editor_pts.clone(),
            win_eq_band_curves: win_eq_band_curves.clone(),
            win_timer: win_timer.clone(),
            project_session: project_session.clone(),
            project_chains: project_chains.clone(),
            project_runtime: project_runtime.clone(),
            saved_project_snapshot: saved_project_snapshot.clone(),
            project_dirty: project_dirty.clone(),
            input_chain_devices: input_chain_devices.clone(),
            output_chain_devices: output_chain_devices.clone(),
            vst3_editor_handles,
            vst3_sample_rate,
            auto_save,
        },
    );

    block_editor_window_lifecycle::wire(
        &win,
        weak_main_window,
        block_editor_window_lifecycle::BlockEditorWindowLifecycleCtx {
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
        },
    );

    Ok((win, block_stream_timer))
}

// `ParameterSet` is referenced by `editor_data.params` and consumed by
// helper functions; the import is kept here so the dependency is explicit.
const _: fn() = || {
    let _ = std::marker::PhantomData::<ParameterSet>;
};
