//! `on_select_chain_block` — dispatch entry for clicking a block in a chain.
//!
//! Branches based on the kind of block at the resolved real-index:
//!
//! * `Input` / `Output` — opens the corresponding I/O groups window with
//!   draft entries cloned from this specific block (so editing one Input
//!   block doesn't touch the other ones in the chain).
//! * `Insert` — opens the insert configuration window seeded with the
//!   block's send/return endpoints. The `show_block_controls` flag depends
//!   on whether the block is in the middle of the chain.
//! * VST3 effect — opens the native plugin GUI directly; no Slint editor
//!   shows up.
//! * Other effect blocks — populate the main `AppWindow`'s shared editor
//!   models, then either:
//!     - inline mode: set `show_block_drawer = true` and start a utility
//!       stream timer (only for utility blocks).
//!     - detached mode: hand off to `block_editor_window_setup::create_and_wire`
//!       which builds a fresh `BlockEditorWindow` with its own per-window
//!       models and wires the parameter + lifecycle callbacks.
//!
//! Wired once from `run_desktop_app`.

use std::cell::RefCell;
use std::rc::Rc;

use slint::{ComponentHandle, ModelRc, SharedString, Timer, VecModel};

use infra_cpal::{AudioDeviceDescriptor, ProjectRuntimeController};
use project::block::AudioBlockKind;

use crate::audio_devices::{
    build_insert_return_channel_items, build_insert_send_channel_items,
    refresh_input_devices, refresh_output_devices, replace_channel_options,
    selected_device_index,
};
use crate::block_editor::{block_editor_data, block_parameter_items_for_editor, build_knob_overlays};
use crate::block_editor_window_setup;
use crate::chain_editor::{chain_draft_from_chain, insert_mode_to_index};
use crate::eq::{build_curve_editor_points, build_multi_slider_points, compute_eq_curves};
use crate::helpers::{set_status_error, show_child_window, use_inline_block_editor};
use crate::io_groups::build_io_group_items;
use crate::project_view::{
    block_model_picker_items, block_model_picker_labels, block_model_index, block_type_index,
    block_type_picker_items, set_selected_block,
};
use crate::runtime_lifecycle::ui_index_to_real_block_index;
use crate::state::{
    BlockEditorDraft, BlockWindow, ChainDraft, InputGroupDraft, InsertDraft, OutputGroupDraft,
    ProjectSession, SelectedBlock,
};
use crate::ui_state::block_drawer_state;
use crate::{
    AppWindow, BlockModelPickerItem, BlockParameterItem, BlockStreamData, BlockStreamEntry,
    BlockTypePickerItem, ChainInputGroupsWindow, ChainInsertWindow, ChainOutputGroupsWindow,
    ChannelOptionItem, CurveEditorPoint, MultiSliderPoint, PluginInfoWindow, ProjectChainItem,
};

pub(crate) struct SelectChainBlockCallbackCtx {
    pub selected_block: Rc<RefCell<Option<SelectedBlock>>>,
    pub block_editor_draft: Rc<RefCell<Option<BlockEditorDraft>>>,
    pub chain_draft: Rc<RefCell<Option<ChainDraft>>>,
    pub insert_draft: Rc<RefCell<Option<InsertDraft>>>,
    pub block_type_options: Rc<VecModel<BlockTypePickerItem>>,
    pub block_model_options: Rc<VecModel<BlockModelPickerItem>>,
    pub filtered_block_model_options: Rc<VecModel<BlockModelPickerItem>>,
    pub block_model_option_labels: Rc<VecModel<SharedString>>,
    pub block_parameter_items: Rc<VecModel<BlockParameterItem>>,
    pub multi_slider_points: Rc<VecModel<MultiSliderPoint>>,
    pub curve_editor_points: Rc<VecModel<CurveEditorPoint>>,
    pub eq_band_curves: Rc<VecModel<SharedString>>,
    pub project_session: Rc<RefCell<Option<ProjectSession>>>,
    pub project_chains: Rc<VecModel<ProjectChainItem>>,
    pub project_runtime: Rc<RefCell<Option<ProjectRuntimeController>>>,
    pub saved_project_snapshot: Rc<RefCell<Option<String>>>,
    pub project_dirty: Rc<RefCell<bool>>,
    pub input_chain_devices: Rc<RefCell<Vec<AudioDeviceDescriptor>>>,
    pub output_chain_devices: Rc<RefCell<Vec<AudioDeviceDescriptor>>>,
    pub chain_input_device_options: Rc<VecModel<SharedString>>,
    pub chain_output_device_options: Rc<VecModel<SharedString>>,
    pub insert_send_channels: Rc<VecModel<ChannelOptionItem>>,
    pub insert_return_channels: Rc<VecModel<ChannelOptionItem>>,
    pub open_block_windows: Rc<RefCell<Vec<BlockWindow>>>,
    pub inline_stream_timer: Rc<RefCell<Option<Timer>>>,
    pub toast_timer: Rc<Timer>,
    pub plugin_info_window: Rc<RefCell<Option<PluginInfoWindow>>>,
    pub vst3_editor_handles: Rc<RefCell<Vec<Box<dyn project::vst3_editor::PluginEditorHandle>>>>,
    pub vst3_sample_rate: f64,
    pub auto_save: bool,
}

pub(crate) fn wire(
    window: &AppWindow,
    chain_input_groups_window: &ChainInputGroupsWindow,
    chain_output_groups_window: &ChainOutputGroupsWindow,
    chain_insert_window: &ChainInsertWindow,
    ctx: SelectChainBlockCallbackCtx,
) {
    let SelectChainBlockCallbackCtx {
        selected_block,
        block_editor_draft,
        chain_draft,
        insert_draft,
        block_type_options,
        block_model_options,
        filtered_block_model_options,
        block_model_option_labels,
        block_parameter_items,
        multi_slider_points,
        curve_editor_points,
        eq_band_curves,
        project_session,
        project_chains,
        project_runtime,
        saved_project_snapshot,
        project_dirty,
        input_chain_devices,
        output_chain_devices,
        chain_input_device_options,
        chain_output_device_options,
        insert_send_channels,
        insert_return_channels,
        open_block_windows,
        inline_stream_timer,
        toast_timer,
        plugin_info_window,
        vst3_editor_handles,
        vst3_sample_rate,
        auto_save,
    } = ctx;

    let weak_main_window = window.as_weak();
    let weak_input_groups = chain_input_groups_window.as_weak();
    let weak_output_groups = chain_output_groups_window.as_weak();
    let weak_insert_window = chain_insert_window.as_weak();

    window.on_select_chain_block(move |chain_index, ui_block_index| {
        let Some(window) = weak_main_window.upgrade() else {
            return;
        };
        let session_borrow = project_session.borrow();
        let Some(session) = session_borrow.as_ref() else {
            set_status_error(&window, &toast_timer, "Nenhum projeto carregado.");
            return;
        };
        let Some(chain) = session.project.chains.get(chain_index as usize) else {
            set_status_error(&window, &toast_timer, "Chain inválida.");
            return;
        };
        // Convert UI index (position in filtered array without first Input/last Output)
        // to real index in chain.blocks — always computed from current chain state
        let block_index = ui_index_to_real_block_index(chain, ui_block_index as usize) as i32;
        log::info!("[select_chain_block] ui_index={} → real_index={}", ui_block_index, block_index);
        let Some(block) = chain.blocks.get(block_index as usize) else {
            log::warn!("[select_chain_block] block_index={} out of bounds, chain has {} blocks", block_index, chain.blocks.len());
            set_status_error(&window, &toast_timer, "Block inválido.");
            return;
        };
        // Handle I/O blocks — open I/O groups window with entries of THIS specific block
        match &block.kind {
            AudioBlockKind::Input(ib) => {
                let fresh_input = refresh_input_devices(&chain_input_device_options);
                let fresh_output = refresh_output_devices(&chain_output_device_options);
                let inputs: Vec<InputGroupDraft> = ib.entries.iter().map(|e| InputGroupDraft {
                    device_id: if e.device_id.0.is_empty() { None } else { Some(e.device_id.0.clone()) },
                    channels: e.channels.clone(),
                    mode: e.mode,
                }).collect();
                let mut draft = chain_draft_from_chain(chain_index as usize, chain);
                draft.inputs = inputs;
                draft.editing_io_block_index = Some(block_index as usize);
                let (input_items, _) = build_io_group_items(&draft, &fresh_input, &fresh_output);
                if let Some(gw) = weak_input_groups.upgrade() {
                    gw.set_groups(ModelRc::from(Rc::new(VecModel::from(input_items))));
                    gw.set_status_message("".into());
                    gw.set_show_block_controls(true);
                    gw.set_block_enabled(block.enabled);
                    *chain_draft.borrow_mut() = Some(draft);
                    drop(session_borrow);
                    show_child_window(window.window(), gw.window());
                }
                return;
            }
            AudioBlockKind::Output(ob) => {
                let fresh_input = refresh_input_devices(&chain_input_device_options);
                let fresh_output = refresh_output_devices(&chain_output_device_options);
                let outputs: Vec<OutputGroupDraft> = ob.entries.iter().map(|e| OutputGroupDraft {
                    device_id: if e.device_id.0.is_empty() { None } else { Some(e.device_id.0.clone()) },
                    channels: e.channels.clone(),
                    mode: e.mode,
                }).collect();
                let mut draft = chain_draft_from_chain(chain_index as usize, chain);
                draft.outputs = outputs;
                draft.editing_io_block_index = Some(block_index as usize);
                let (_, output_items) = build_io_group_items(&draft, &fresh_input, &fresh_output);
                if let Some(gw) = weak_output_groups.upgrade() {
                    gw.set_groups(ModelRc::from(Rc::new(VecModel::from(output_items))));
                    gw.set_status_message("".into());
                    gw.set_show_block_controls(true);
                    gw.set_block_enabled(block.enabled);
                    *chain_draft.borrow_mut() = Some(draft);
                    drop(session_borrow);
                    show_child_window(window.window(), gw.window());
                }
                return;
            }
            AudioBlockKind::Insert(ib) => {
                let fresh_input = refresh_input_devices(&chain_input_device_options);
                let fresh_output = refresh_output_devices(&chain_output_device_options);
                log::info!("[select_chain_block] insert block at index {}: id='{}'", block_index, block.id.0);
                let draft = InsertDraft {
                    chain_index: chain_index as usize,
                    block_index: block_index as usize,
                    send_device_id: if ib.send.device_id.0.is_empty() { None } else { Some(ib.send.device_id.0.clone()) },
                    send_channels: ib.send.channels.clone(),
                    send_mode: ib.send.mode,
                    return_device_id: if ib.return_.device_id.0.is_empty() { None } else { Some(ib.return_.device_id.0.clone()) },
                    return_channels: ib.return_.channels.clone(),
                    return_mode: ib.return_.mode,
                };
                let is_middle = block_index > 0 && (block_index as usize) < chain.blocks.len() - 1;
                if let Some(iw) = weak_insert_window.upgrade() {
                    let send_items = build_insert_send_channel_items(&draft, &fresh_output);
                    let return_items = build_insert_return_channel_items(&draft, &fresh_input);
                    replace_channel_options(&insert_send_channels, send_items);
                    replace_channel_options(&insert_return_channels, return_items);
                    iw.set_selected_send_device_index(selected_device_index(
                        &fresh_output,
                        draft.send_device_id.as_deref(),
                    ));
                    iw.set_selected_return_device_index(selected_device_index(
                        &fresh_input,
                        draft.return_device_id.as_deref(),
                    ));
                    iw.set_selected_send_mode_index(insert_mode_to_index(draft.send_mode));
                    iw.set_selected_return_mode_index(insert_mode_to_index(draft.return_mode));
                    iw.set_show_block_controls(is_middle);
                    iw.set_block_enabled(block.enabled);
                    iw.set_status_message("".into());
                    *insert_draft.borrow_mut() = Some(draft);
                    drop(session_borrow);
                    show_child_window(window.window(), iw.window());
                }
                return;
            }
            _ => {}
        }
        log::info!("[select_chain_block] block at real_index={}: id='{}', kind={}", block_index, block.id.0, block.model_ref().map(|m| format!("{}/{}", m.effect_type, m.model)).unwrap_or_else(|| "io/insert".to_string()));
        log::info!("[select_chain_block] chain has {} blocks:", chain.blocks.len());
        for (i, b) in chain.blocks.iter().enumerate() {
            log::info!("[select_chain_block]   [{}] id='{}' kind={}", i, b.id.0, b.model_ref().map(|m| format!("{}/{}", m.effect_type, m.model)).unwrap_or_else(|| "io/insert".to_string()));
        }
        let Some(editor_data) = block_editor_data(block) else {
            set_status_error(&window, &toast_timer, "Esse block ainda não pode ser editado pela GUI.");
            return;
        };
        let effect_type = editor_data.effect_type.clone();
        let model_id = editor_data.model_id.clone();
        let enabled = editor_data.enabled;
        *selected_block.borrow_mut() = Some(SelectedBlock {
            chain_index: chain_index as usize,
            block_index: block_index as usize,
        });
        let instrument = chain.instrument.clone();
        log::info!("[select_chain_block] chain_index={}, block_index={}, effect_type='{}', model_id='{}', enabled={}", chain_index, block_index, effect_type, model_id, enabled);
        *block_editor_draft.borrow_mut() = Some(BlockEditorDraft {
            chain_index: chain_index as usize,
            block_index: Some(block_index as usize),
            before_index: block_index as usize,
            instrument: instrument.clone(),
            effect_type: effect_type.clone(),
            model_id: model_id.clone(),
            enabled,
            is_select: editor_data.is_select,
        });
        let items = block_model_picker_items(&effect_type, &instrument);
        log::debug!("[select_chain_block] filtered models count={}", items.len());
        for item in &items {
            log::trace!("[select_chain_block]   model='{}'", item.model_id);
        }
        block_model_option_labels.set_vec(block_model_picker_labels(&items));
        block_model_options.set_vec(items.clone());
        filtered_block_model_options.set_vec(items);
        block_parameter_items.set_vec(block_parameter_items_for_editor(&editor_data));
        multi_slider_points.set_vec(build_multi_slider_points(&editor_data.effect_type, &editor_data.model_id, &editor_data.params));
        curve_editor_points.set_vec(build_curve_editor_points(&editor_data.effect_type, &editor_data.model_id, &editor_data.params));
        let (eq_total, eq_bands) = compute_eq_curves(&editor_data.effect_type, &editor_data.model_id, &editor_data.params);
        eq_band_curves.set_vec(eq_bands.into_iter().map(SharedString::from).collect::<Vec<_>>());
        window.set_eq_total_curve(eq_total.into());
        set_selected_block(&window, selected_block.borrow().as_ref(), Some(chain));
        let drawer_state =
            block_drawer_state(Some(block_index as usize), &effect_type, Some(&model_id));
        window.set_block_drawer_title(drawer_state.title.into());
        window.set_block_drawer_confirm_label(drawer_state.confirm_label.into());
        window.set_block_drawer_edit_mode(true);
        block_type_options.set_vec(block_type_picker_items(&instrument));
        window.set_block_drawer_selected_type_index(block_type_index(&effect_type, &instrument));
        window
            .set_block_drawer_selected_model_index(block_model_index(&effect_type, &model_id, &instrument));
        window.set_block_drawer_enabled(enabled);
        window.set_block_drawer_status_message("".into());
        window.set_show_block_type_picker(false);
        // Clone block_id before dropping session_borrow (needed by window editor stream timer)
        let block_id_for_editor = block.id.clone();
        let is_vst3_block = effect_type == block_core::EFFECT_TYPE_VST3;
        drop(session_borrow);
        // VST3 blocks: open the native plugin GUI directly — no Slint editor popup.
        if is_vst3_block && !model_id.is_empty() {
            match project::vst3_editor::open_vst3_editor(&model_id, vst3_sample_rate) {
                Ok(handle) => { vst3_editor_handles.borrow_mut().push(handle); }
                Err(e) => set_status_error(&window, &toast_timer, &format!("Erro ao abrir plugin VST3: {}", e)),
            }
            return;
        }
        if use_inline_block_editor(&window) {
            let param_items_vec = block_parameter_items_for_editor(&editor_data);
            let overlays = build_knob_overlays(project::catalog::model_knob_layout(&effect_type, &model_id), &param_items_vec);
            window.set_block_knob_overlays(ModelRc::from(Rc::new(VecModel::from(overlays))));
            // Start inline stream timer for utility blocks (tuner, spectrum analyzer)
            {
                let mut timer_ref = inline_stream_timer.borrow_mut();
                *timer_ref = None; // stop previous timer
                let is_utility = effect_type == block_core::EFFECT_TYPE_UTILITY;
                if is_utility {
                    let timer = Timer::default();
                    let weak_win = window.as_weak();
                    let runtime = project_runtime.clone();
                    let bid = block_id_for_editor.clone();
                    timer.start(
                        slint::TimerMode::Repeated,
                        std::time::Duration::from_millis(50),
                        move || {
                            let Some(win) = weak_win.upgrade() else { return; };
                            let runtime_borrow = runtime.borrow();
                            // No utility block currently produces a "spectrum" stream
                            // (the spectrum_analyzer block was promoted to a top-bar
                            // feature in #320). Kept generic for future stream blocks.
                            let kind: slint::SharedString = "stream".into();
                            let Some(rt) = runtime_borrow.as_ref() else { return; };
                            if let Some(entries) = rt.poll_stream(&bid) {
                                let slint_entries: Vec<BlockStreamEntry> = entries.iter().map(|e| BlockStreamEntry {
                                    key: e.key.clone().into(),
                                    value: e.value,
                                    text: e.text.clone().into(),
                                    peak: e.peak,
                                }).collect();
                                win.set_block_stream_data(BlockStreamData {
                                    active: true,
                                    stream_kind: kind,
                                    entries: ModelRc::from(Rc::new(VecModel::from(slint_entries))),
                                });
                            } else {
                                win.set_block_stream_data(BlockStreamData {
                                    active: false,
                                    stream_kind: kind,
                                    entries: ModelRc::default(),
                                });
                            }
                        },
                    );
                    *timer_ref = Some(timer);
                }
            }
            window.set_show_block_drawer(true);
        } else {
            window.set_show_block_drawer(false);
            let ci = chain_index as usize;
            let bi = block_index as usize;
            // If this block already has an open editor, bring it to front.
            {
                let borrow = open_block_windows.borrow();
                if let Some(bw) = borrow.iter().find(|bw| bw.chain_index == ci && bw.block_index == bi) {
                    show_child_window(window.window(), bw.window.window());
                    return;
                }
            }
            // Close any stale window for this block position before creating a fresh one.
            // After add/remove operations the block at a given index may have changed.
            {
                let borrow = open_block_windows.borrow();
                for bw in borrow.iter().filter(|bw| bw.chain_index == ci && bw.block_index == bi) {
                    let _ = bw.window.hide();
                }
            }
            open_block_windows.borrow_mut().retain(|bw| {
                !(bw.chain_index == ci && bw.block_index == bi)
            });
            // Build + wire a fresh BlockEditorWindow for this block
            let setup_ctx = block_editor_window_setup::BlockEditorWindowSetupCtx {
                chain_index: ci,
                block_index: bi,
                instrument: instrument.clone(),
                effect_type: effect_type.clone(),
                model_id: model_id.clone(),
                enabled,
                editor_data,
                block_id: block_id_for_editor,
                project_session: project_session.clone(),
                project_chains: project_chains.clone(),
                project_runtime: project_runtime.clone(),
                saved_project_snapshot: saved_project_snapshot.clone(),
                project_dirty: project_dirty.clone(),
                input_chain_devices: input_chain_devices.clone(),
                output_chain_devices: output_chain_devices.clone(),
                selected_block: selected_block.clone(),
                open_block_windows: open_block_windows.clone(),
                plugin_info_window: plugin_info_window.clone(),
                vst3_editor_handles: vst3_editor_handles.clone(),
                vst3_sample_rate,
                auto_save,
            };
            let (win, block_stream_timer) = match block_editor_window_setup::create_and_wire(
                weak_main_window.clone(),
                setup_ctx,
            ) {
                Ok(pair) => pair,
                Err(e) => {
                    set_status_error(&window, &toast_timer, &format!("Erro ao abrir editor: {e}"));
                    return;
                }
            };
            show_child_window(window.window(), win.window());
            open_block_windows.borrow_mut().push(BlockWindow { chain_index: ci, block_index: bi, window: win, stream_timer: block_stream_timer });
        }
    });
}
