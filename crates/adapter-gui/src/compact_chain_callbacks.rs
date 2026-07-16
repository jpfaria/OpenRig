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

use application::dispatcher::CommandDispatcher;

use application::di_loader::DiLoopSource;

use crate::compact_block_view::build_compact_blocks;
use crate::compact_chain_block_handlers::{self, CompactChainBlockHandlersCtx};
use crate::compact_chain_param_handlers::{self, CompactChainParamHandlersCtx};
use crate::helpers::{set_status_error, show_child_window};
use crate::project_view::{block_type_picker_items, real_block_index_to_ui};
use crate::state::{BlockEditorDraft, ProjectSession};
use crate::{
    AppWindow, BlockStreamData, BlockStreamEntry, CompactChainViewWindow, ProjectChainItem,
};

// ── #614: public play/stop entry points for the compact chain view ──────────
//
// These are thin wrappers around `di_loop_wiring::play_chain_di_loop` /
// `stop_chain_di_loop`. Exposed as `pub` so integration tests can call them
// directly without going through `AppWindow` (same pattern the chain-row
// wiring uses via `di_loop_wiring::*`).

/// Arm the DI loop for `chain` from the compact chain view.
///
/// Delegates to `di_loop_wiring::play_chain_di_loop` — same dispatch +
/// runtime-apply path the main chains screen uses.
pub fn compact_chain_di_loop_play(
    project_runtime: &std::cell::RefCell<Option<infra_cpal::ProjectRuntimeController>>,
    dispatcher: &application::local_dispatcher::LocalDispatcher,
    chain: &domain::ids::ChainId,
) {
    crate::di_loop_wiring::play_chain_di_loop(project_runtime, dispatcher, chain);
}

/// Disarm the DI loop for `chain` from the compact chain view.
///
/// Delegates to `di_loop_wiring::stop_chain_di_loop`.
pub fn compact_chain_di_loop_stop(
    project_runtime: &std::cell::RefCell<Option<infra_cpal::ProjectRuntimeController>>,
    dispatcher: &application::local_dispatcher::LocalDispatcher,
    chain: &domain::ids::ChainId,
) {
    crate::di_loop_wiring::stop_chain_di_loop(project_runtime, dispatcher, chain);
}

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
    pub block_editor_draft: Rc<RefCell<Option<BlockEditorDraft>>>,
    pub fullscreen: bool,
    pub auto_save: bool,
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
        block_editor_draft,
        fullscreen,
        auto_save,
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
            set_status_error(
                &window,
                &toast_timer,
                &rust_i18n::t!("error-no-project-loaded"),
            );
            return;
        };
        let ci = chain_index as usize;
        // #591: if this chain's compact view is already open, focus it
        // rather than stacking a second window — a footswitch
        // (`toggle_compact_view`) can re-trigger this for the active chain.
        if let Some((open_ci, weak)) = &*open_compact_window.borrow() {
            if *open_ci == ci {
                if let Some(existing) = weak.upgrade() {
                    let _ = existing.show();
                    return;
                }
            }
        }
        let compact_win = match CompactChainViewWindow::new() {
            Ok(w) => w,
            Err(e) => {
                log::error!("failed to create compact chain view: {e}");
                return;
            }
        };
        {
            let proj = session.project.borrow();
            let Some(chain) = proj.chains.get(ci) else {
                set_status_error(&window, &toast_timer, &rust_i18n::t!("error-invalid-chain"));
                return;
            };
            let title = chain
                .description
                .clone()
                .unwrap_or_else(|| rust_i18n::t!("default-chain-name", n = ci + 1).to_string());
            compact_win.set_chain_title(title.into());
            compact_win.set_chain_index(chain_index);
            compact_win.set_chain_enabled(chain.enabled);
            compact_win.set_block_type_options(ModelRc::from(Rc::new(VecModel::from(
                block_type_picker_items(&chain.instrument),
            ))));
        }
        let blocks = build_compact_blocks(&session.project.borrow(), ci);
        let compact_blocks = Rc::new(VecModel::from(blocks));
        compact_win.set_compact_blocks(ModelRc::from(compact_blocks.clone()));
        drop(session_borrow);

        // Store weak ref for refresh after block insert/save
        *open_compact_window.borrow_mut() = Some((ci, compact_win.as_weak()));

        // Wire search-block-model: update filtered_models inside the
        // CompactBlockItem at (ci, bi). MUST read the live model via
        // `cw.get_compact_blocks()` on every invocation — block CRUD and
        // param updates call `set_compact_blocks(...)` and REPLACE the
        // underlying VecModel, so capturing the original `Rc<VecModel>`
        // by move leaves the handler bound to an orphaned model and the
        // popup stops filtering after the first model change (#538).
        {
            let weak_compact = compact_win.as_weak();
            compact_win.on_search_block_model(move |ci, bi, text| {
                log::debug!(
                    "[search-compact] callback received: ci={} bi={} text={:?}",
                    ci,
                    bi,
                    text
                );
                let Some(cw) = weak_compact.upgrade() else {
                    return;
                };
                let live = cw.get_compact_blocks();
                let Some(vm) = live
                    .as_any()
                    .downcast_ref::<VecModel<crate::CompactBlockItem>>()
                else {
                    log::warn!(
                        "[search-compact] live compact_blocks is not a VecModel; \
                         search ignored"
                    );
                    return;
                };
                crate::model_search_wiring::refilter_compact_block(vm, ci, bi, text.as_str());
            });
        }

        // Wire choose-block-model-by-id: resolve model_id to its index
        // within the block's full models list, then forward to the
        // existing index-based handler. Same live-model rule as the
        // search handler — capturing the original Rc breaks model
        // resolution after the first model change (#538).
        {
            let weak_compact = compact_win.as_weak();
            compact_win.on_choose_block_model_by_id(move |ci, bi, model_id| {
                let Some(cw) = weak_compact.upgrade() else {
                    return;
                };
                let live = cw.get_compact_blocks();
                let Some(vm) = live
                    .as_any()
                    .downcast_ref::<VecModel<crate::CompactBlockItem>>()
                else {
                    return;
                };
                let Some(idx) = crate::model_search_wiring::resolve_model_id_in_compact_block(
                    vm,
                    ci,
                    bi,
                    model_id.as_str(),
                ) else {
                    log::warn!(
                        "[compact] choose-model-by-id: model_id '{}' not found in \
                         block (chain={}, block={}) — nothing dispatched",
                        model_id,
                        ci,
                        bi
                    );
                    return;
                };
                cw.invoke_choose_block_model(ci, bi, idx);
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

        // Header admin forwarders + #787 parameter tabs / drag geometry.
        crate::compact_chain_header_wiring::wire(
            &window,
            &compact_win,
            chain_index,
            &project_session,
        );

        // Issue #360: chain delete renders its overlay INSIDE the compact view.
        crate::compact_chain_delete_wiring::wire(
            &window,
            &compact_win,
            crate::compact_chain_delete_wiring::CompactChainDeleteCtx {
                project_session: project_session.clone(),
                project_chains: project_chains.clone(),
                project_runtime: project_runtime.clone(),
                saved_project_snapshot: saved_project_snapshot.clone(),
                project_dirty: project_dirty.clone(),
                input_chain_devices: input_chain_devices.clone(),
                output_chain_devices: output_chain_devices.clone(),
                toast_timer: toast_timer.clone(),
                auto_save,
            },
        );

        crate::compact_chain_header_wiring::start_header_poll(&window, &compact_win, ci);

        // Wire choose-block-type — when user picks a type from the compact view picker
        {
            let weak_main = window.as_weak();
            compact_win.on_choose_block_type(move |ci, before, type_index| {
                log::debug!(
                    "[compact] choose-block-type: chain={}, before={}, type_index={}",
                    ci,
                    before,
                    type_index
                );
                let Some(main_win) = weak_main.upgrade() else {
                    return;
                };
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
                let Some(main_win) = weak_main.upgrade() else {
                    return;
                };
                // bi is a real block index from CompactBlockItem — convert to UI index
                // because on_select_chain_block now expects UI indices
                let session_borrow = project_session_detail.borrow();
                let ui_bi = if let Some(session) = session_borrow.as_ref() {
                    let proj = session.project.borrow();
                    if let Some(chain) = proj.chains.get(ci as usize) {
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
                    let Some(cw) = weak_cw.upgrade() else {
                        return;
                    };
                    let rt_borrow = project_runtime_poll.borrow();
                    let Some(rt) = rt_borrow.as_ref() else {
                        return;
                    };
                    let compact_blocks = cw.get_compact_blocks();
                    for i in 0..compact_blocks.row_count() {
                        if let Some(mut item) = compact_blocks.row_data(i) {
                            if item.effect_type == "utility" {
                                let stream_data = if item.enabled {
                                    let bid = BlockId(item.block_id.to_string());
                                    let kind: slint::SharedString =
                                        project::catalog::model_stream_kind(
                                            item.effect_type.as_str(),
                                            item.model_id.as_str(),
                                        )
                                        .into();
                                    if let Some(entries) = rt.poll_stream(&bid) {
                                        let slint_entries: Vec<BlockStreamEntry> = entries
                                            .iter()
                                            .map(|e| BlockStreamEntry {
                                                key: e.key.clone().into(),
                                                value: e.value,
                                                text: e.text.clone().into(),
                                                peak: e.peak,
                                            })
                                            .collect();
                                        BlockStreamData {
                                            active: true,
                                            stream_kind: kind,
                                            entries: ModelRc::from(Rc::new(VecModel::from(
                                                slint_entries,
                                            ))),
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
                                    BlockStreamData {
                                        active: false,
                                        stream_kind: "".into(),
                                        entries: ModelRc::default(),
                                    }
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

        // ── #614: DI loop callbacks (compact view) ───────────────────────────
        // The compact view exposes a ChainDiLoopButton next to the volume
        // control. Its 4 callbacks target the focused chain (`chain_index`
        // captured from the outer closure).  All delegate to the same helpers
        // the chains-screen tile wiring uses — no duplicate dispatch path.

        // on_di_loop_source_selected: user picked a bundled source.
        {
            let project_session = project_session.clone();
            let weak_window = window.as_weak();
            let toast_timer = toast_timer.clone();
            compact_win.on_di_loop_source_selected(move |source_str| {
                let chain_id = {
                    let session_borrow = project_session.borrow();
                    let Some(session) = session_borrow.as_ref() else {
                        return;
                    };
                    let proj = session.project.borrow();
                    let Some(chain) = proj.chains.get(chain_index as usize) else {
                        return;
                    };
                    chain.id.clone()
                };
                let source = DiLoopSource::Bundled(source_str.to_string());
                let cmds = crate::di_loop_wiring::di_loop_commands(
                    chain_id,
                    crate::di_loop_wiring::DiLoopIntent::SelectSource { source },
                );
                let session_borrow = project_session.borrow();
                let Some(session) = session_borrow.as_ref() else {
                    return;
                };
                for cmd in cmds {
                    if let Err(err) = session.dispatcher.dispatch(cmd) {
                        if let Some(main_win) = weak_window.upgrade() {
                            set_status_error(&main_win, &toast_timer, &err.to_string());
                        }
                        return;
                    }
                }
            });
        }

        // #771 on_di_loop_output_selected: user picked an output endpoint.
        crate::di_output_select_wiring::wire_compact(
            &compact_win,
            chain_index,
            project_session.clone(),
            project_runtime.clone(),
        );

        // on_di_loop_choose_file: user picked "Choose file…" — open native dialog.
        {
            let project_session = project_session.clone();
            let weak_window = window.as_weak();
            let toast_timer = toast_timer.clone();
            compact_win.on_di_loop_choose_file(move || {
                let chain_id = {
                    let session_borrow = project_session.borrow();
                    let Some(session) = session_borrow.as_ref() else {
                        return;
                    };
                    let proj = session.project.borrow();
                    let Some(chain) = proj.chains.get(chain_index as usize) else {
                        return;
                    };
                    chain.id.clone()
                };
                let Some(path) = rfd::FileDialog::new()
                    .add_filter("WAV audio", &["wav"])
                    .pick_file()
                else {
                    return; // user cancelled
                };
                let cmds = crate::di_loop_wiring::di_loop_commands(
                    chain_id,
                    crate::di_loop_wiring::DiLoopIntent::SelectSource {
                        source: DiLoopSource::File(path),
                    },
                );
                let session_borrow = project_session.borrow();
                let Some(session) = session_borrow.as_ref() else {
                    return;
                };
                for cmd in cmds {
                    if let Err(err) = session.dispatcher.dispatch(cmd) {
                        if let Some(main_win) = weak_window.upgrade() {
                            set_status_error(&main_win, &toast_timer, &err.to_string());
                        }
                        return;
                    }
                }
            });
        }

        // on_di_loop_play: user pressed ▶ in the compact view.
        {
            let project_session = project_session.clone();
            let project_runtime = project_runtime.clone();
            compact_win.on_di_loop_play(move || {
                let chain_id = {
                    let session_borrow = project_session.borrow();
                    let Some(session) = session_borrow.as_ref() else {
                        return;
                    };
                    let proj = session.project.borrow();
                    let Some(chain) = proj.chains.get(chain_index as usize) else {
                        return;
                    };
                    chain.id.clone()
                };
                let session_borrow = project_session.borrow();
                let Some(session) = session_borrow.as_ref() else {
                    return;
                };
                compact_chain_di_loop_play(&project_runtime, &session.dispatcher, &chain_id);
            });
        }

        // on_di_loop_stop: user pressed ■ in the compact view.
        {
            let project_session = project_session.clone();
            let project_runtime = project_runtime.clone();
            compact_win.on_di_loop_stop(move || {
                let chain_id = {
                    let session_borrow = project_session.borrow();
                    let Some(session) = session_borrow.as_ref() else {
                        return;
                    };
                    let proj = session.project.borrow();
                    let Some(chain) = proj.chains.get(chain_index as usize) else {
                        return;
                    };
                    chain.id.clone()
                };
                let session_borrow = project_session.borrow();
                let Some(session) = session_borrow.as_ref() else {
                    return;
                };
                compact_chain_di_loop_stop(&project_runtime, &session.dispatcher, &chain_id);
            });
        }

        show_child_window(window.window(), compact_win.window());
    });
}
