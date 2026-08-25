//! Wiring for the chain Insert block editor window (`ChainInsertWindow`).
//!
//! An insert is an external send/return loop, and since #716 (model A) all it
//! carries is the E/S binding that loop runs through: the SEND goes out that
//! binding's output, the RETURN comes back on its input. So this window offers
//! that single pick — device / mode / channels belong to the E/S itself and are
//! edited in the I/O bindings screen.

use std::cell::RefCell;
use std::rc::Rc;

use slint::{ComponentHandle, ModelRc, SharedString, VecModel};

use application::command::{BlockCommand, Command};
use domain::AudioDeviceDescriptor;
use infra_filesystem::IoBinding;

use crate::port_wiring::{binding_options, session_registry};
use crate::project_ops::sync_project_dirty;
use crate::project_view::replace_project_chains;
use crate::runtime_sync_policy::request_chain_sync;
use crate::state::{InsertDraft, ProjectSession};
use crate::{AppWindow, ChainInsertWindow, ProjectChainItem};

/// State borrowed by the Insert window callbacks. Each `Rc` is cloned per
/// callback closure that needs it.
pub(crate) struct InsertWiringCtx {
    pub insert_draft: Rc<RefCell<Option<InsertDraft>>>,
    pub input_chain_devices: Rc<RefCell<Vec<AudioDeviceDescriptor>>>,
    pub output_chain_devices: Rc<RefCell<Vec<AudioDeviceDescriptor>>>,
    pub project_session: Rc<RefCell<Option<ProjectSession>>>,
    pub project_chains: Rc<VecModel<ProjectChainItem>>,
    pub saved_project_snapshot: Rc<RefCell<Option<String>>>,
    pub project_dirty: Rc<RefCell<bool>>,
    pub auto_save: bool,
}

/// Fill the window's E/S select for `draft` and show it.
pub(crate) fn open_insert_window(
    insert_window: &ChainInsertWindow,
    insert_draft: &Rc<RefCell<Option<InsertDraft>>>,
    draft: InsertDraft,
    registry: &[IoBinding],
    enabled: bool,
) {
    let selected = registry.iter().position(|b| b.id == draft.io);
    insert_window.set_binding_options(fresh_options(binding_options(registry)));
    insert_window.set_selected_binding_index(selected.map_or(-1, |i| i as i32));
    insert_window.set_block_enabled(enabled);
    insert_window.set_show_binding_warning(false);
    *insert_draft.borrow_mut() = Some(draft);
    let _ = insert_window.show();
}

/// Hand the select a BRAND-NEW model instead of rewriting the rows of the one
/// it already holds — a `ComboBox` only recomputes its text when `model` or
/// `current-index` changes (same reason as the port editor, #85).
fn fresh_options(items: Vec<SharedString>) -> ModelRc<SharedString> {
    ModelRc::from(Rc::new(VecModel::from(items)))
}

pub(crate) fn wire(
    window: &AppWindow,
    chain_insert_window: &ChainInsertWindow,
    ctx: InsertWiringCtx,
) {
    let InsertWiringCtx {
        insert_draft,
        input_chain_devices,
        output_chain_devices,
        project_session,
        project_chains,
        saved_project_snapshot,
        project_dirty,
        auto_save,
    } = ctx;

    // --- pick the E/S the loop runs through ---
    {
        let insert_draft = insert_draft.clone();
        let project_session = project_session.clone();
        let weak_insert_window = chain_insert_window.as_weak();
        chain_insert_window.on_select_binding(move |index: i32| {
            let Some(iw) = weak_insert_window.upgrade() else {
                return;
            };
            let registry = session_registry(&project_session.borrow());
            let Some(binding) = registry.get(index.max(0) as usize) else {
                return;
            };
            let mut draft_borrow = insert_draft.borrow_mut();
            let Some(draft) = draft_borrow.as_mut() else {
                return;
            };
            draft.io = binding.id.clone();
            iw.set_show_binding_warning(false);
        });
    }

    // --- enable / disable the insert ---
    {
        let insert_draft = insert_draft.clone();
        let project_session = project_session.clone();
        let project_chains = project_chains.clone();
        let input_chain_devices = input_chain_devices.clone();
        let output_chain_devices = output_chain_devices.clone();
        let saved_project_snapshot = saved_project_snapshot.clone();
        let project_dirty = project_dirty.clone();
        let weak_window = window.as_weak();
        let weak_insert_window = chain_insert_window.as_weak();
        chain_insert_window.on_toggle_enabled(move || {
            let Some(window) = weak_window.upgrade() else {
                return;
            };
            let Some(iw) = weak_insert_window.upgrade() else {
                return;
            };
            let draft_borrow = insert_draft.borrow();
            let Some(draft) = draft_borrow.as_ref() else {
                return;
            };
            let chain_idx = draft.chain_index;
            let block_idx = draft.block_index;
            drop(draft_borrow);
            let mut session_borrow = project_session.borrow_mut();
            let Some(session) = session_borrow.as_mut() else {
                return;
            };
            // Resolve positional indices to IDs before dispatching.
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
            if let Err(e) =
                session
                    .dispatcher
                    .dispatch(Command::Block(BlockCommand::ToggleBlockEnabled {
                        chain: chain_id.clone(),
                        block: block_id,
                    }))
            {
                log::error!("toggle insert block enabled: {e}");
                return;
            }
            let block_enabled = session
                .project
                .borrow()
                .chains
                .iter()
                .find(|c| c.id == chain_id)
                .and_then(|c| c.blocks.get(block_idx))
                .map(|b| b.enabled)
                .unwrap_or(false);
            iw.set_block_enabled(block_enabled);
            // #127: the runtime already knows — `ToggleBlockEnabled` applied the
            // live toggle from the dispatcher (through `RuntimeControl`), so a
            // failure came back out of the dispatch above.
            replace_project_chains(
                &project_chains,
                &session.project.borrow(),
                &input_chain_devices.borrow(),
                &output_chain_devices.borrow(),
                &[],
            );
            sync_project_dirty(
                &window,
                session,
                &saved_project_snapshot,
                &project_dirty,
                auto_save,
            );
        });
    }

    // --- delete the insert ---
    {
        let insert_draft = insert_draft.clone();
        let project_session = project_session.clone();
        let project_chains = project_chains.clone();
        let input_chain_devices = input_chain_devices.clone();
        let output_chain_devices = output_chain_devices.clone();
        let saved_project_snapshot = saved_project_snapshot.clone();
        let project_dirty = project_dirty.clone();
        let weak_window = window.as_weak();
        let weak_insert_window = chain_insert_window.as_weak();
        chain_insert_window.on_delete_block(move || {
            let Some(window) = weak_window.upgrade() else {
                return;
            };
            let Some(iw) = weak_insert_window.upgrade() else {
                return;
            };
            let draft_borrow = insert_draft.borrow();
            let Some(draft) = draft_borrow.as_ref() else {
                return;
            };
            let chain_idx = draft.chain_index;
            let block_idx = draft.block_index;
            drop(draft_borrow);
            *insert_draft.borrow_mut() = None;
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
                .dispatch(Command::Block(BlockCommand::RemoveBlock {
                    chain: chain_id.clone(),
                    block: block_id,
                }))
            {
                log::error!("delete insert block: {e}");
                return;
            }
            if let Err(e) = request_chain_sync(session, &chain_id) {
                log::error!("delete insert block: {e}");
            }
            replace_project_chains(
                &project_chains,
                &session.project.borrow(),
                &input_chain_devices.borrow(),
                &output_chain_devices.borrow(),
                &[],
            );
            sync_project_dirty(
                &window,
                session,
                &saved_project_snapshot,
                &project_dirty,
                auto_save,
            );
            let _ = iw.hide();
        });
    }

    // --- OK: bind the insert to the picked E/S ---
    {
        let insert_draft = insert_draft.clone();
        let project_session = project_session.clone();
        let project_chains = project_chains.clone();
        let input_chain_devices = input_chain_devices.clone();
        let output_chain_devices = output_chain_devices.clone();
        let saved_project_snapshot = saved_project_snapshot.clone();
        let project_dirty = project_dirty.clone();
        let weak_window = window.as_weak();
        let weak_insert_window = chain_insert_window.as_weak();
        chain_insert_window.on_save(move || {
            let Some(window) = weak_window.upgrade() else {
                return;
            };
            let Some(iw) = weak_insert_window.upgrade() else {
                return;
            };
            let draft_borrow = insert_draft.borrow();
            let Some(draft) = draft_borrow.as_ref() else {
                return;
            };
            let chain_idx = draft.chain_index;
            let block_idx = draft.block_index;
            let io = draft.io.clone();
            drop(draft_borrow);
            // An insert with no E/S has nowhere to send and nothing to return:
            // saving it that way is what left the chain silent (#881). Say so
            // and keep the window open instead of writing an unbound loop.
            if io.is_empty() {
                iw.set_show_binding_warning(true);
                return;
            }
            *insert_draft.borrow_mut() = None;
            let mut session_borrow = project_session.borrow_mut();
            let Some(session) = session_borrow.as_mut() else {
                let _ = iw.hide();
                return;
            };
            // Resolve positional indices to IDs before dispatching.
            let (chain_id, block_id) = {
                let proj = session.project.borrow();
                let Some(chain) = proj.chains.get(chain_idx) else {
                    let _ = iw.hide();
                    return;
                };
                let Some(block) = chain.blocks.get(block_idx) else {
                    let _ = iw.hide();
                    return;
                };
                (chain.id.clone(), block.id.clone())
            };
            if let Err(e) =
                session
                    .dispatcher
                    .dispatch(Command::Block(BlockCommand::SaveInsertBlock {
                        chain: chain_id.clone(),
                        block: block_id,
                        io,
                    }))
            {
                log::error!("insert save error: {e}");
                let _ = iw.hide();
                return;
            }
            if let Err(e) = request_chain_sync(session, &chain_id) {
                log::error!("insert save runtime sync error: {e}");
            }
            replace_project_chains(
                &project_chains,
                &session.project.borrow(),
                &input_chain_devices.borrow(),
                &output_chain_devices.borrow(),
                &[],
            );
            sync_project_dirty(
                &window,
                session,
                &saved_project_snapshot,
                &project_dirty,
                auto_save,
            );
            let _ = iw.hide();
        });
    }

    // --- cancel ---
    {
        let insert_draft = insert_draft.clone();
        let weak_insert_window = chain_insert_window.as_weak();
        chain_insert_window.on_cancel(move || {
            *insert_draft.borrow_mut() = None;
            if let Some(iw) = weak_insert_window.upgrade() {
                let _ = iw.hide();
            }
        });
    }
}
