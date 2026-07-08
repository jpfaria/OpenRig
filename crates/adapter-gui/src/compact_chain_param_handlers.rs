//! Compact chain view — block parameter update callbacks.
//!
//! Three near-identical handlers driving live parameter changes from the
//! compact chain view: numeric knobs, enum options (dropdowns), and boolean
//! toggles (e.g. mute_signal). Each writes the value into the block's params,
//! rebuilds the AudioBlockKind via `project::block::build_audio_block_kind`,
//! resyncs the live runtime, and refreshes both the compact view rows and the
//! project dirty marker.
//!
//! Called once per compact view instance from `compact_chain_callbacks::wire`.

use std::cell::RefCell;
use std::rc::Rc;

use slint::{ComponentHandle, ModelRc, Timer, VecModel};

use application::command::Command;
use application::dispatcher::CommandDispatcher;
use infra_cpal::{AudioDeviceDescriptor, ProjectRuntimeController};

use crate::helpers::set_status_error;
use crate::project_ops::sync_project_dirty;
use crate::project_view::{build_compact_blocks, replace_project_chains};
use crate::state::ProjectSession;
use crate::sync_live_chain_runtime;
use crate::{AppWindow, CompactChainViewWindow, ProjectChainItem};

pub(crate) struct CompactChainParamHandlersCtx {
    pub project_session: Rc<RefCell<Option<ProjectSession>>>,
    pub project_runtime: Rc<RefCell<Option<ProjectRuntimeController>>>,
    pub project_chains: Rc<VecModel<ProjectChainItem>>,
    pub input_chain_devices: Rc<RefCell<Vec<AudioDeviceDescriptor>>>,
    pub output_chain_devices: Rc<RefCell<Vec<AudioDeviceDescriptor>>>,
    pub saved_project_snapshot: Rc<RefCell<Option<String>>>,
    pub project_dirty: Rc<RefCell<bool>>,
    pub toast_timer: Rc<Timer>,
    pub auto_save: bool,
}

pub(crate) fn wire(
    main_window: &AppWindow,
    compact_win: &CompactChainViewWindow,
    ctx: CompactChainParamHandlersCtx,
) {
    let CompactChainParamHandlersCtx {
        project_session,
        project_runtime,
        project_chains,
        input_chain_devices,
        output_chain_devices,
        saved_project_snapshot,
        project_dirty,
        toast_timer,
        auto_save,
    } = ctx;

    // Wire update-block-parameter-number (knobs)
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
        compact_win.on_update_block_parameter_number(move |ci, bi, path, value| {
            let Some(main_win) = weak_main.upgrade() else {
                return;
            };
            let Some(cw) = weak_compact.upgrade() else {
                return;
            };
            let chain_idx = ci as usize;
            let block_idx = bi as usize;
            let mut session_borrow = project_session.borrow_mut();
            let Some(session) = session_borrow.as_mut() else {
                return;
            };
            // Resolve IDs (read-only) before dispatching.
            let (chain_id, block_id) = {
                let proj = session.project.borrow();
                let Some(chain) = proj.chains.get(chain_idx) else {
                    return;
                };
                let Some(block) = chain.blocks.get(block_idx) else {
                    return;
                };
                (chain.id.clone(), block.id.clone())
            };
            if let Err(e) = session
                .dispatcher
                .dispatch(Command::SetBlockParameterNumber {
                    chain: chain_id.clone(),
                    block: block_id,
                    path: path.to_string(),
                    value: value as f64,
                })
            {
                log::error!("[compact] update param error: {e}");
                return;
            }
            let chain_id = chain_id;
            if let Err(e) = sync_live_chain_runtime(&project_runtime, session, &chain_id) {
                set_status_error(&main_win, &toast_timer, &e.to_string());
                return;
            }
            replace_project_chains(
                &project_chains,
                &*session.project.borrow(),
                &*input_chain_devices.borrow(),
                &*output_chain_devices.borrow(),
                &[],
            );
            let blocks = build_compact_blocks(&*session.project.borrow(), chain_idx);
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

    // Wire select-block-parameter-option (enums)
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
        compact_win.on_select_block_parameter_option(move |ci, bi, path, option_index| {
            let Some(main_win) = weak_main.upgrade() else {
                return;
            };
            let Some(cw) = weak_compact.upgrade() else {
                return;
            };
            let chain_idx = ci as usize;
            let block_idx = bi as usize;
            let mut session_borrow = project_session.borrow_mut();
            let Some(session) = session_borrow.as_mut() else {
                return;
            };
            // Resolve IDs and option value string (read-only) before dispatching.
            let (chain_id, block_id, option_value) = {
                let proj = session.project.borrow();
                let Some(chain) = proj.chains.get(chain_idx) else {
                    return;
                };
                let Some(block) = chain.blocks.get(block_idx) else {
                    return;
                };
                // Resolve option value string from schema + index.
                let Some(data) = crate::block_editor::block_editor_data(block) else {
                    return;
                };
                let schema =
                    match project::block::schema_for_block_model(&data.effect_type, &data.model_id)
                    {
                        Ok(s) => s,
                        Err(_) => return,
                    };
                let Some(param_spec) = schema.parameters.iter().find(|p| p.path == path.as_str())
                else {
                    return;
                };
                let option_value = match &param_spec.domain {
                    block_core::param::ParameterDomain::Enum { options } => {
                        options.get(option_index as usize).map(|o| o.value.clone())
                    }
                    _ => None,
                };
                let Some(value) = option_value else {
                    return;
                };
                (chain.id.clone(), block.id.clone(), value)
            };
            if let Err(e) = session
                .dispatcher
                .dispatch(Command::SelectBlockParameterOption {
                    chain: chain_id.clone(),
                    block: block_id,
                    path: path.to_string(),
                    value: option_value,
                    index: option_index as usize,
                })
            {
                log::error!("[compact] select option error: {e}");
                return;
            }
            let chain_id = chain_id;
            if let Err(e) = sync_live_chain_runtime(&project_runtime, session, &chain_id) {
                set_status_error(&main_win, &toast_timer, &e.to_string());
                return;
            }
            replace_project_chains(
                &project_chains,
                &*session.project.borrow(),
                &*input_chain_devices.borrow(),
                &*output_chain_devices.borrow(),
                &[],
            );
            let blocks = build_compact_blocks(&*session.project.borrow(), chain_idx);
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

    // Wire update-block-parameter-bool (bool toggles like mute_signal)
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
        compact_win.on_update_block_parameter_bool(move |ci, bi, path, value| {
            let Some(main_win) = weak_main.upgrade() else {
                return;
            };
            let Some(cw) = weak_compact.upgrade() else {
                return;
            };
            let chain_idx = ci as usize;
            let block_idx = bi as usize;
            let mut session_borrow = project_session.borrow_mut();
            let Some(session) = session_borrow.as_mut() else {
                return;
            };
            // Resolve IDs (read-only) before dispatching.
            let (chain_id, block_id) = {
                let proj = session.project.borrow();
                let Some(chain) = proj.chains.get(chain_idx) else {
                    return;
                };
                let Some(block) = chain.blocks.get(block_idx) else {
                    return;
                };
                (chain.id.clone(), block.id.clone())
            };
            if let Err(e) = session.dispatcher.dispatch(Command::SetBlockParameterBool {
                chain: chain_id.clone(),
                block: block_id,
                path: path.to_string(),
                value,
            }) {
                log::error!("[compact] update bool param error: {e}");
                return;
            }
            let chain_id = chain_id;
            if let Err(e) = sync_live_chain_runtime(&project_runtime, session, &chain_id) {
                set_status_error(&main_win, &toast_timer, &e.to_string());
                return;
            }
            replace_project_chains(
                &project_chains,
                &*session.project.borrow(),
                &*input_chain_devices.borrow(),
                &*output_chain_devices.borrow(),
                &[],
            );
            let blocks = build_compact_blocks(&*session.project.borrow(), chain_idx);
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
}
