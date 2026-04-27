//! Compact chain view — block CRUD callbacks.
//!
//! Handles per-block manipulation from the compact chain view: enable toggle,
//! chain-level enable toggle (with async JACK startup on Linux), model swap,
//! remove and reorder. Each handler keeps the live runtime, project YAML
//! snapshot, and dirty flag in sync, then rebuilds the compact_blocks model
//! so the UI reflects the new state.
//!
//! Called once per compact view instance from `compact_chain_callbacks::wire`.

use std::cell::RefCell;
use std::rc::Rc;

use slint::{ComponentHandle, Model, ModelRc, Timer, VecModel};

use infra_cpal::{AudioDeviceDescriptor, ProjectRuntimeController};

use crate::block_editor::block_editor_data;
use crate::helpers::set_status_error;
#[cfg(target_os = "linux")]
use crate::helpers::set_status_info;
use crate::project_ops::sync_project_dirty;
use crate::project_view::{block_model_picker_items, build_compact_blocks, replace_project_chains};
use crate::state::{BlockEditorDraft, ProjectSession};
use crate::sync_live_chain_runtime;
use crate::{AppWindow, CompactChainViewWindow, ProjectChainItem};

pub(crate) struct CompactChainBlockHandlersCtx {
    pub project_session: Rc<RefCell<Option<ProjectSession>>>,
    pub project_runtime: Rc<RefCell<Option<ProjectRuntimeController>>>,
    pub project_chains: Rc<VecModel<ProjectChainItem>>,
    pub input_chain_devices: Rc<RefCell<Vec<AudioDeviceDescriptor>>>,
    pub output_chain_devices: Rc<RefCell<Vec<AudioDeviceDescriptor>>>,
    pub saved_project_snapshot: Rc<RefCell<Option<String>>>,
    pub project_dirty: Rc<RefCell<bool>>,
    pub block_editor_draft: Rc<RefCell<Option<BlockEditorDraft>>>,
    pub toast_timer: Rc<Timer>,
    pub auto_save: bool,
}

pub(crate) fn wire(
    main_window: &AppWindow,
    compact_win: &CompactChainViewWindow,
    ctx: CompactChainBlockHandlersCtx,
) {
    let CompactChainBlockHandlersCtx {
        project_session,
        project_runtime,
        project_chains,
        input_chain_devices,
        output_chain_devices,
        saved_project_snapshot,
        project_dirty,
        block_editor_draft,
        toast_timer,
        auto_save,
    } = ctx;

    // Wire toggle-enabled callback
    {
        let project_session = project_session.clone();
        let project_runtime = project_runtime.clone();
        let project_chains = project_chains.clone();
        let input_chain_devices = input_chain_devices.clone();
        let output_chain_devices = output_chain_devices.clone();
        let saved_project_snapshot = saved_project_snapshot.clone();
        let project_dirty = project_dirty.clone();
        let block_editor_draft = block_editor_draft.clone();
        let weak_main = main_window.as_weak();
        let weak_compact = compact_win.as_weak();
        let toast_timer = toast_timer.clone();
        compact_win.on_toggle_block_enabled(move |ci, bi| {
            let Some(main_win) = weak_main.upgrade() else { return; };
            let Some(cw) = weak_compact.upgrade() else { return; };
            let mut session_borrow = project_session.borrow_mut();
            let Some(session) = session_borrow.as_mut() else { return; };
            let chain_idx = ci as usize;
            let block_idx = bi as usize;
            let Some(chain) = session.project.chains.get_mut(chain_idx) else { return; };
            let Some(block) = chain.blocks.get_mut(block_idx) else { return; };
            block.enabled = !block.enabled;
            let new_enabled = block.enabled;
            let chain_id = chain.id.clone();
            // Keep block_editor_draft in sync to prevent stale persist from reverting
            if let Some(draft) = block_editor_draft.borrow_mut().as_mut() {
                if draft.chain_index == chain_idx && draft.block_index == Some(block_idx) {
                    draft.enabled = new_enabled;
                }
            }
            if let Err(error) = sync_live_chain_runtime(&project_runtime, session, &chain_id) {
                set_status_error(&main_win, &toast_timer, &error.to_string());
                return;
            }
            replace_project_chains(
                &project_chains,
                &session.project,
                &*input_chain_devices.borrow(),
                &*output_chain_devices.borrow(),
            );
            // Refresh compact blocks
            let blocks = build_compact_blocks(&session.project, chain_idx);
            cw.set_compact_blocks(ModelRc::from(Rc::new(VecModel::from(blocks))));
            sync_project_dirty(
                &main_win,
                session,
                &saved_project_snapshot,
                &project_dirty,
                auto_save,
            );
        });
    }

    // Wire toggle-chain-enabled
    {
        let project_session = project_session.clone();
        let project_runtime = project_runtime.clone();
        let project_chains = project_chains.clone();
        let input_chain_devices = input_chain_devices.clone();
        let output_chain_devices = output_chain_devices.clone();
        let weak_main = main_window.as_weak();
        let weak_compact = compact_win.as_weak();
        let toast_timer = toast_timer.clone();
        // Guard against double-clicks while JACK is starting up
        let jack_starting: Rc<std::cell::Cell<bool>> = Rc::new(std::cell::Cell::new(false));
        compact_win.on_toggle_chain_enabled(move |ci| {
            let Some(main_win) = weak_main.upgrade() else { return; };
            let Some(cw) = weak_compact.upgrade() else { return; };
            // Block re-entrant clicks during async JACK startup
            if jack_starting.get() { return; }
            let mut session_borrow = project_session.borrow_mut();
            let Some(session) = session_borrow.as_mut() else { return; };
            let chain_idx = ci as usize;
            let Some(chain) = session.project.chains.get_mut(chain_idx) else { return; };
            let will_enable = !chain.enabled;
            chain.enabled = will_enable;
            let chain_id = chain.id.clone();

            // On Linux: always start JACK asynchronously when enabling a chain.
            // This prevents blocking the UI thread regardless of whether JACK
            // is already running or needs to be started.
            #[cfg(target_os = "linux")]
            if will_enable {
                // Pass the full per-device settings list; the background
                // thread will look up each card's configuration by
                // device_id when launching jackd for it.
                let device_settings = session.project.device_settings.clone();
                drop(session_borrow);

                jack_starting.set(true);
                set_status_info(&main_win, &toast_timer, "Starting audio...");

                let rx = Rc::new(std::cell::RefCell::new(
                    infra_cpal::start_jack_in_background(device_settings),
                ));
                let project_session_t = project_session.clone();
                let project_runtime_t = project_runtime.clone();
                let project_chains_t = project_chains.clone();
                let input_chain_devices_t = input_chain_devices.clone();
                let output_chain_devices_t = output_chain_devices.clone();
                let weak_main_t = weak_main.clone();
                let weak_compact_t = weak_compact.clone();
                let toast_timer_t = toast_timer.clone();
                let jack_starting_t = jack_starting.clone();
                let done = Rc::new(std::cell::Cell::new(false));
                let done_t = done.clone();
                let poll_timer = slint::Timer::default();
                poll_timer.start(
                    slint::TimerMode::Repeated,
                    std::time::Duration::from_millis(300),
                    move || {
                        if done_t.get() { return; }
                        use std::sync::mpsc::TryRecvError;
                        match rx.borrow().try_recv() {
                            Err(TryRecvError::Empty) => return,
                            Err(TryRecvError::Disconnected) => {
                                done_t.set(true);
                                jack_starting_t.set(false);
                                return;
                            }
                            Ok(Err(e)) => {
                                done_t.set(true);
                                jack_starting_t.set(false);
                                if let Some(win) = weak_main_t.upgrade() {
                                    set_status_error(&win, &toast_timer_t, &e.to_string());
                                }
                                // Revert chain.enabled
                                let mut sb = project_session_t.borrow_mut();
                                if let Some(s) = sb.as_mut() {
                                    if let Some(c) = s.project.chains.iter_mut().find(|c| c.id == chain_id) {
                                        c.enabled = false;
                                    }
                                }
                                return;
                            }
                            Ok(Ok(())) => {
                                done_t.set(true);
                                jack_starting_t.set(false);
                                let Some(win) = weak_main_t.upgrade() else { return; };
                                let Some(cw) = weak_compact_t.upgrade() else { return; };
                                let mut sb = project_session_t.borrow_mut();
                                let Some(session) = sb.as_mut() else { return; };
                                if let Err(e) = sync_live_chain_runtime(&project_runtime_t, session, &chain_id) {
                                    set_status_error(&win, &toast_timer_t, &e.to_string());
                                    if let Some(c) = session.project.chains.iter_mut().find(|c| c.id == chain_id) {
                                        c.enabled = false;
                                    }
                                } else {
                                    replace_project_chains(&project_chains_t, &session.project, &*input_chain_devices_t.borrow(), &*output_chain_devices_t.borrow());
                                    cw.set_chain_enabled(true);
                                    set_status_info(&win, &toast_timer_t, "");
                                }
                            }
                        }
                    },
                );
                std::mem::forget(poll_timer);
                return;
            }

            if let Err(error) = sync_live_chain_runtime(&project_runtime, session, &chain_id) {
                set_status_error(&main_win, &toast_timer, &error.to_string());
                return;
            }
            replace_project_chains(&project_chains, &session.project, &*input_chain_devices.borrow(), &*output_chain_devices.borrow());
            cw.set_chain_enabled(will_enable);
        });
    }

    // Wire choose-block-model
    {
        let project_session = project_session.clone();
        let project_runtime = project_runtime.clone();
        let project_chains = project_chains.clone();
        let input_chain_devices = input_chain_devices.clone();
        let output_chain_devices = output_chain_devices.clone();
        let saved_project_snapshot = saved_project_snapshot.clone();
        let project_dirty = project_dirty.clone();
        let weak_main = main_window.as_weak();
        let weak_compact = compact_win.as_weak();
        let toast_timer = toast_timer.clone();
        compact_win.on_choose_block_model(move |ci, bi, mi| {
            let Some(main_win) = weak_main.upgrade() else { return; };
            let Some(cw) = weak_compact.upgrade() else { return; };
            let chain_idx = ci as usize;
            let block_idx = bi as usize;
            let model_idx = mi as usize;

            // Get the instrument to filter models
            let instrument = {
                let session_borrow = project_session.borrow();
                let Some(session) = session_borrow.as_ref() else { return; };
                let Some(chain) = session.project.chains.get(chain_idx) else { return; };
                chain.instrument.clone()
            };

            // Get the effect type from the current block
            let effect_type = {
                let session_borrow = project_session.borrow();
                let Some(session) = session_borrow.as_ref() else { return; };
                let Some(chain) = session.project.chains.get(chain_idx) else { return; };
                let Some(block) = chain.blocks.get(block_idx) else { return; };
                let Some(data) = block_editor_data(block) else { return; };
                data.effect_type.clone()
            };

            let models = block_model_picker_items(&effect_type, &instrument);
            let Some(model) = models.get(model_idx) else { return; };
            let new_model_id = model.model_id.to_string();
            let new_effect_type = model.effect_type.to_string();

            // Build new block kind with default params
            let new_params = block_core::param::ParameterSet::default();
            let schema = match project::block::schema_for_block_model(&new_effect_type, &new_model_id) {
                Ok(s) => s,
                Err(e) => {
                    log::error!("compact choose-model schema error: {e}");
                    return;
                }
            };
            let normalized = match new_params.normalized_against(&schema) {
                Ok(p) => p,
                Err(e) => {
                    log::error!("compact choose-model normalize error: {e}");
                    return;
                }
            };
            let kind = match project::block::build_audio_block_kind(&new_effect_type, &new_model_id, normalized) {
                Ok(k) => k,
                Err(e) => {
                    log::error!("compact choose-model build error: {e}");
                    return;
                }
            };

            // Apply to project
            let mut session_borrow = project_session.borrow_mut();
            let Some(session) = session_borrow.as_mut() else { return; };
            let Some(chain) = session.project.chains.get_mut(chain_idx) else { return; };
            let Some(block) = chain.blocks.get_mut(block_idx) else { return; };
            let block_id = block.id.clone();
            let enabled = block.enabled;
            block.kind = kind;
            block.id = block_id;
            block.enabled = enabled;

            let chain_id = chain.id.clone();
            if let Err(error) = sync_live_chain_runtime(&project_runtime, session, &chain_id) {
                set_status_error(&main_win, &toast_timer, &error.to_string());
                return;
            }
            replace_project_chains(&project_chains, &session.project, &*input_chain_devices.borrow(), &*output_chain_devices.borrow());
            let blocks = build_compact_blocks(&session.project, chain_idx);
            cw.set_compact_blocks(ModelRc::from(Rc::new(VecModel::from(blocks))));
            sync_project_dirty(&main_win, session, &saved_project_snapshot, &project_dirty, auto_save);
        });
    }

    // Wire remove-block
    {
        let project_session = project_session.clone();
        let weak_main = main_window.as_weak();
        let weak_compact = compact_win.as_weak();
        let project_chains = project_chains.clone();
        let project_runtime = project_runtime.clone();
        let saved_project_snapshot = saved_project_snapshot.clone();
        let project_dirty = project_dirty.clone();
        let input_chain_devices = input_chain_devices.clone();
        let output_chain_devices = output_chain_devices.clone();
        let _toast_timer = toast_timer.clone();
        compact_win.on_remove_block(move |ci, bi| {
            log::info!("[compact] remove-block: chain={}, block={}", ci, bi);
            let Some(main_win) = weak_main.upgrade() else { return; };
            let Some(cw) = weak_compact.upgrade() else { return; };
            let mut session_borrow = project_session.borrow_mut();
            let Some(session) = session_borrow.as_mut() else { return; };
            let chain_idx = ci as usize;
            let block_idx = bi as usize;
            let Some(chain) = session.project.chains.get_mut(chain_idx) else { return; };
            if block_idx >= chain.blocks.len() { return; }
            chain.blocks.remove(block_idx);
            let chain_id = chain.id.clone();
            if let Err(e) = sync_live_chain_runtime(&project_runtime, session, &chain_id) {
                log::error!("[compact] remove-block runtime sync: {}", e);
            }
            replace_project_chains(&project_chains, &session.project, &*input_chain_devices.borrow(), &*output_chain_devices.borrow());
            let blocks = build_compact_blocks(&session.project, chain_idx);
            cw.set_compact_blocks(ModelRc::from(Rc::new(VecModel::from(blocks))));
            sync_project_dirty(&main_win, session, &saved_project_snapshot, &project_dirty, auto_save);
        });
    }

    // Wire reorder-block — resolve real indices from CompactBlockItem.block_index
    {
        let project_session = project_session.clone();
        let weak_main = main_window.as_weak();
        let weak_compact = compact_win.as_weak();
        let project_chains = project_chains.clone();
        let project_runtime = project_runtime.clone();
        let saved_project_snapshot = saved_project_snapshot.clone();
        let project_dirty = project_dirty.clone();
        let input_chain_devices = input_chain_devices.clone();
        let output_chain_devices = output_chain_devices.clone();
        compact_win.on_reorder_block(move |ci, compact_from, compact_before| {
            let Some(main_win) = weak_main.upgrade() else { return; };
            let Some(cw) = weak_compact.upgrade() else { return; };
            // Look up real chain.blocks indices from the Slint compact model
            let compact_model = cw.get_compact_blocks();
            let compact_len = compact_model.row_count();
            let from_pos = compact_from as usize;
            if from_pos >= compact_len { return; }
            let from_index = compact_model.row_data(from_pos)
                .map(|item| item.block_index as usize)
                .unwrap_or(0);
            let before_pos = compact_before as usize;
            let real_before = if before_pos < compact_len {
                compact_model.row_data(before_pos)
                    .map(|item| item.block_index as usize)
                    .unwrap_or(0)
            } else {
                // "after last compact block" → one position after last compact block's real index
                compact_model.row_data(compact_len - 1)
                    .map(|item| item.block_index as usize + 1)
                    .unwrap_or(0)
            };
            log::info!("[compact] reorder-block: compact_from={}, compact_before={}, real_from={}, real_before={}", compact_from, compact_before, from_index, real_before);
            if real_before == from_index || real_before == from_index + 1 { return; }
            let mut session_borrow = project_session.borrow_mut();
            let Some(session) = session_borrow.as_mut() else { return; };
            let chain_idx = ci as usize;
            let Some(chain) = session.project.chains.get_mut(chain_idx) else { return; };
            let block_count = chain.blocks.len();
            if from_index >= block_count { return; }
            let block = chain.blocks.remove(from_index);
            let mut normalized_before = real_before;
            if normalized_before > from_index { normalized_before -= 1; }
            let insert_at = normalized_before.min(chain.blocks.len());
            chain.blocks.insert(insert_at, block);
            let chain_id = chain.id.clone();
            if let Err(e) = sync_live_chain_runtime(&project_runtime, session, &chain_id) {
                log::error!("[compact] reorder-block runtime sync: {}", e);
            }
            replace_project_chains(&project_chains, &session.project, &*input_chain_devices.borrow(), &*output_chain_devices.borrow());
            let blocks = build_compact_blocks(&session.project, chain_idx);
            cw.set_compact_blocks(ModelRc::from(Rc::new(VecModel::from(blocks))));
            sync_project_dirty(&main_win, session, &saved_project_snapshot, &project_dirty, auto_save);
        });
    }
}

