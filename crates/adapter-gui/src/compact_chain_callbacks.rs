//! `on_open_compact_chain_view` — entry point for the compact chain view.
//!
//! Creates a per-chain `CompactChainViewWindow` on demand, populates its
//! state from the active project session, and wires the callbacks that drive
//! it. Block CRUD and per-block parameter updates are delegated to
//! `compact_chain_block_handlers` and `compact_chain_param_handlers` to keep
//! each concern in its own ≤500-line module. The remaining handlers stay
//! here because they are closely tied to the window's lifetime: search /
//! choose-by-id (depend on the compact_blocks model created here),
//! configure-input/output and choose-block-type (forwarders to the main
//! window), open-block-detail (translates a real block index back to the UI
//! index), open-plugin (VST3 editor handle bookkeeping), close, and the
//! stream-polling timer that drives utility-block visualizations.
//!
//! Wired once from `run_desktop_app`.

use std::cell::RefCell;
use std::rc::Rc;

use slint::{ComponentHandle, Model, ModelRc, Timer, VecModel, Weak};

use domain::ids::BlockId;
use infra_cpal::{AudioDeviceDescriptor, ProjectRuntimeController};

use crate::compact_chain_block_handlers::{self, CompactChainBlockHandlersCtx};
use crate::compact_chain_param_handlers::{self, CompactChainParamHandlersCtx};
use crate::helpers::{set_status_error, show_child_window};
use crate::project_view::{
    block_type_picker_items, build_compact_blocks, real_block_index_to_ui,
};
use crate::state::{BlockEditorDraft, ProjectSession};
use crate::{
    AppWindow, BlockStreamData, BlockStreamEntry, CompactChainViewWindow, ProjectChainItem,
};

pub(crate) struct CompactChainCallbacksCtx {
    pub project_session: Rc<RefCell<Option<ProjectSession>>>,
    pub project_runtime: Rc<RefCell<Option<ProjectRuntimeController>>>,
    pub project_chains: Rc<VecModel<ProjectChainItem>>,
    pub input_chain_devices: Rc<RefCell<Vec<AudioDeviceDescriptor>>>,
    pub output_chain_devices: Rc<RefCell<Vec<AudioDeviceDescriptor>>>,
    pub saved_project_snapshot: Rc<RefCell<Option<String>>>,
    pub project_dirty: Rc<RefCell<bool>>,
    pub toast_timer: Rc<Timer>,
    pub open_compact_window: Rc<RefCell<Option<(usize, Weak<CompactChainViewWindow>)>>>,
    pub vst3_editor_handles: Rc<RefCell<Vec<Box<dyn project::vst3_editor::PluginEditorHandle>>>>,
    pub block_editor_draft: Rc<RefCell<Option<BlockEditorDraft>>>,
    pub fullscreen: bool,
    pub auto_save: bool,
    pub vst3_sample_rate: f64,
}

pub(crate) fn wire(window: &AppWindow, ctx: CompactChainCallbacksCtx) {
    let CompactChainCallbacksCtx {
        project_session,
        project_runtime,
        project_chains,
        input_chain_devices,
        output_chain_devices,
        saved_project_snapshot,
        project_dirty,
        toast_timer,
        open_compact_window,
        vst3_editor_handles,
        block_editor_draft,
        fullscreen,
        auto_save,
        vst3_sample_rate,
    } = ctx;

    let weak_window = window.as_weak();
    window.on_open_compact_chain_view(move |chain_index| {
        let Some(window) = weak_window.upgrade() else {
            return;
        };
        // In fullscreen mode, compact view is not available
        if fullscreen {
            return;
        }
        let session_borrow = project_session.borrow();
        let Some(session) = session_borrow.as_ref() else {
            set_status_error(&window, &toast_timer, &rust_i18n::t!("Nenhum projeto carregado."));
            return;
        };
        let ci = chain_index as usize;
        let Some(chain) = session.project.chains.get(ci) else {
            set_status_error(&window, &toast_timer, &rust_i18n::t!("Chain inválida."));
            return;
        };

        let compact_win = match CompactChainViewWindow::new() {
            Ok(w) => w,
            Err(e) => {
                log::error!("failed to create compact chain view: {e}");
                return;
            }
        };
        let title = chain
            .description
            .clone()
            .unwrap_or_else(|| format!("Chain {}", ci + 1));
        compact_win.set_chain_title(title.into());
        compact_win.set_chain_index(chain_index);
        compact_win.set_chain_enabled(chain.enabled);
        compact_win.set_block_type_options(ModelRc::from(Rc::new(VecModel::from(
            block_type_picker_items(&chain.instrument),
        ))));

        let blocks = build_compact_blocks(&session.project, ci);
        let compact_blocks = Rc::new(VecModel::from(blocks));
        compact_win.set_compact_blocks(ModelRc::from(compact_blocks.clone()));
        drop(session_borrow);

        // Store weak ref for refresh after block insert/save
        *open_compact_window.borrow_mut() = Some((ci, compact_win.as_weak()));

        // Wire search-block-model: update filtered_models inside the
        // CompactBlockItem at (ci, bi).
        {
            let compact_blocks = compact_blocks.clone();
            compact_win.on_search_block_model(move |ci, bi, text| {
                log::debug!(
                    "[search-compact] callback received: ci={} bi={} text={:?}",
                    ci,
                    bi,
                    text
                );
                crate::model_search_wiring::refilter_compact_block(
                    &compact_blocks,
                    ci,
                    bi,
                    text.as_str(),
                );
            });
        }

        // Wire choose-block-model-by-id: resolve model_id to its index
        // within the block's full models list, then forward to the
        // existing index-based handler.
        {
            let compact_blocks = compact_blocks.clone();
            let weak_compact = compact_win.as_weak();
            compact_win.on_choose_block_model_by_id(move |ci, bi, model_id| {
                let Some(idx) = crate::model_search_wiring::resolve_model_id_in_compact_block(
                    &compact_blocks,
                    ci,
                    bi,
                    model_id.as_str(),
                ) else {
                    return;
                };
                if let Some(cw) = weak_compact.upgrade() {
                    cw.invoke_choose_block_model(ci, bi, idx);
                }
            });
        }

        // Block CRUD + chain enable callbacks (extracted module)
        compact_chain_block_handlers::wire(
            &window,
            &compact_win,
            CompactChainBlockHandlersCtx {
                project_session: project_session.clone(),
                project_runtime: project_runtime.clone(),
                project_chains: project_chains.clone(),
                input_chain_devices: input_chain_devices.clone(),
                output_chain_devices: output_chain_devices.clone(),
                saved_project_snapshot: saved_project_snapshot.clone(),
                project_dirty: project_dirty.clone(),
                block_editor_draft: block_editor_draft.clone(),
                toast_timer: toast_timer.clone(),
                auto_save,
            },
        );

        // Per-block parameter update callbacks (extracted module)
        compact_chain_param_handlers::wire(
            &window,
            &compact_win,
            CompactChainParamHandlersCtx {
                project_session: project_session.clone(),
                project_runtime: project_runtime.clone(),
                project_chains: project_chains.clone(),
                input_chain_devices: input_chain_devices.clone(),
                output_chain_devices: output_chain_devices.clone(),
                saved_project_snapshot: saved_project_snapshot.clone(),
                project_dirty: project_dirty.clone(),
                toast_timer: toast_timer.clone(),
                auto_save,
            },
        );

        // Wire close callback
        {
            let weak_compact = compact_win.as_weak();
            compact_win.on_close_compact_view(move || {
                if let Some(cw) = weak_compact.upgrade() {
                    cw.hide().ok();
                }
            });
        }

        // Wire choose-block-type — when user picks a type from the compact view picker
        {
            let weak_main = window.as_weak();
            compact_win.on_choose_block_type(move |ci, before, type_index| {
                log::debug!("[compact] choose-block-type: chain={}, before={}, type_index={}", ci, before, type_index);
                let Some(main_win) = weak_main.upgrade() else { return; };
                // Trigger the full insert flow on the main window (sets up draft + opens editor)
                main_win.invoke_start_block_insert(ci, before);
                // Select the type that was chosen
                main_win.invoke_choose_block_type(type_index);
            });
        }

        // Wire open-block-detail (click on model select opens full editor)
        {
            let weak_main = window.as_weak();
            let project_session_detail = project_session.clone();
            compact_win.on_open_block_detail(move |ci, bi| {
                let Some(main_win) = weak_main.upgrade() else { return; };
                // bi is a real block index from CompactBlockItem — convert to UI index
                // because on_select_chain_block now expects UI indices
                let session_borrow = project_session_detail.borrow();
                let ui_bi = if let Some(session) = session_borrow.as_ref() {
                    if let Some(chain) = session.project.chains.get(ci as usize) {
                        real_block_index_to_ui(chain, bi as usize)
                            .map(|i| i as i32)
                            .unwrap_or(bi)
                    } else {
                        bi
                    }
                } else {
                    bi
                };
                main_win.invoke_select_chain_block(ci, ui_bi);
                let _ = main_win.window().show();
            });
        }

        // Stream polling timer — updates stream_data for enabled utility blocks
        {
            let weak_cw = compact_win.as_weak();
            let project_runtime_poll = project_runtime.clone();
            let stream_timer = Timer::default();
            stream_timer.start(
                slint::TimerMode::Repeated,
                std::time::Duration::from_millis(80),
                move || {
                    let Some(cw) = weak_cw.upgrade() else { return; };
                    let rt_borrow = project_runtime_poll.borrow();
                    let Some(rt) = rt_borrow.as_ref() else { return; };
                    let compact_blocks = cw.get_compact_blocks();
                    for i in 0..compact_blocks.row_count() {
                        if let Some(mut item) = compact_blocks.row_data(i) {
                            if item.effect_type == "utility" {
                                let stream_data = if item.enabled {
                                    let bid = BlockId(item.block_id.to_string());
                                    let kind: slint::SharedString = project::catalog::model_stream_kind(item.effect_type.as_str(), item.model_id.as_str()).into();
                                    if let Some(entries) = rt.poll_stream(&bid) {
                                        let slint_entries: Vec<BlockStreamEntry> = entries.iter().map(|e| BlockStreamEntry {
                                            key: e.key.clone().into(),
                                            value: e.value,
                                            text: e.text.clone().into(),
                                            peak: e.peak,
                                        }).collect();
                                        BlockStreamData {
                                            active: true,
                                            stream_kind: kind,
                                            entries: ModelRc::from(Rc::new(VecModel::from(slint_entries))),
                                        }
                                    } else {
                                        BlockStreamData {
                                            active: false,
                                            stream_kind: kind,
                                            entries: ModelRc::default(),
                                        }
                                    }
                                } else {
                                    // Disabled utility block — clear stream so parameters become visible
                                    BlockStreamData { active: false, stream_kind: "".into(), entries: ModelRc::default() }
                                };
                                item.stream_data = stream_data;
                                compact_blocks.set_row_data(i, item);
                            }
                        }
                    }
                },
            );
            // Timer lives as long as compact_win (dropped when window closes)
            std::mem::forget(stream_timer);
        }

        // Wire configure-input/output — delegate to the main window's existing handlers
        {
            let weak_main = window.as_weak();
            compact_win.on_configure_input(move |ci| {
                log::warn!("[compact] on_configure_input fired, chain_index={}", ci);
                if let Some(main_win) = weak_main.upgrade() {
                    log::warn!("[compact] main_win upgrade OK, invoking configure_chain_input");
                    main_win.invoke_configure_chain_input(ci);
                } else {
                    log::warn!("[compact] main_win upgrade FAILED");
                }
            });
        }
        {
            let weak_main = window.as_weak();
            compact_win.on_configure_output(move |ci| {
                log::warn!("[compact] on_configure_output fired, chain_index={}", ci);
                if let Some(main_win) = weak_main.upgrade() {
                    log::warn!("[compact] main_win upgrade OK, invoking configure_chain_output");
                    main_win.invoke_configure_chain_output(ci);
                } else {
                    log::warn!("[compact] main_win upgrade FAILED");
                }
            });
        }

        {
            let vst3_handles = vst3_editor_handles.clone();
            let vst3_sr = vst3_sample_rate;
            compact_win.on_open_plugin(move |model_id| {
                match project::vst3_editor::open_vst3_editor(model_id.as_str(), vst3_sr) {
                    Ok(handle) => { vst3_handles.borrow_mut().push(handle); }
                    Err(e) => log::error!("[compact] failed to open VST3 editor '{}': {}", model_id, e),
                }
            });
        }

        show_child_window(window.window(), compact_win.window());
    });
}
