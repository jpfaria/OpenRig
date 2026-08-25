//! #85 — the mid-chain I/O port editor (`ChainPortWindow`).
//!
//! A port block (`Input`/`Output` placed between effects) has no model and no
//! parameters: all it carries is WHERE it reads from / writes to — one E/S
//! binding plus one endpoint of it. That is exactly what the model persists
//! (`SaveChainInputEndpoints` / `SaveChainOutputEndpoints`), so this window
//! offers those two choices and nothing else. Device / mode / channels belong
//! to the E/S itself and are edited in the I/O bindings screen.

use std::cell::RefCell;
use std::rc::Rc;

use slint::{ComponentHandle, Model, ModelRc, SharedString, VecModel};

use application::command::{BlockCommand, ChainCommand, Command};
use domain::AudioDeviceDescriptor;
use infra_filesystem::IoBinding;

use crate::project_ops::sync_project_dirty;
use crate::project_view::replace_project_chains;
use crate::runtime_sync_policy::request_chain_sync;
use crate::state::{PortDraft, ProjectSession};
use crate::{AppWindow, ChainPortWindow, ProjectChainItem};

/// The bindings the port may point at, in registry order.
pub(crate) fn binding_options(registry: &[IoBinding]) -> Vec<SharedString> {
    registry
        .iter()
        .map(|b| {
            let name = b.name.trim();
            SharedString::from(if name.is_empty() { &b.id } else { name })
        })
        .collect()
}

/// The endpoints of `binding` in the port's own direction — an input port picks
/// among the binding's inputs, an output port among its outputs.
pub(crate) fn endpoint_options(binding: Option<&IoBinding>, is_input: bool) -> Vec<SharedString> {
    let Some(binding) = binding else {
        return Vec::new();
    };
    let endpoints = if is_input {
        &binding.inputs
    } else {
        &binding.outputs
    };
    endpoints
        .iter()
        .map(|e| SharedString::from(e.name.as_str()))
        .collect()
}

/// Hand a select a BRAND-NEW model instead of rewriting the rows of the one it
/// already holds. A `ComboBox` only recomputes the text it shows when its
/// `model` property or its `current-index` changes; replacing rows in place
/// changes neither, so the box keeps displaying whatever it displayed before —
/// an empty ENDPOINT box for a port whose endpoint sits at the index the select
/// was already on (#85).
fn fresh_options(items: Vec<SharedString>) -> ModelRc<SharedString> {
    ModelRc::from(Rc::new(VecModel::from(items)))
}

pub(crate) struct PortWiringCtx {
    pub port_draft: Rc<RefCell<Option<PortDraft>>>,
    pub project_session: Rc<RefCell<Option<ProjectSession>>>,
    pub project_chains: Rc<VecModel<ProjectChainItem>>,
    pub saved_project_snapshot: Rc<RefCell<Option<String>>>,
    pub project_dirty: Rc<RefCell<bool>>,
    pub input_chain_devices: Rc<RefCell<Vec<AudioDeviceDescriptor>>>,
    pub output_chain_devices: Rc<RefCell<Vec<AudioDeviceDescriptor>>>,
    pub auto_save: bool,
}

/// Fill the window's selects for `draft` and show it.
pub(crate) fn open_port_window(
    port_window: &ChainPortWindow,
    ctx: &PortWiringCtx,
    draft: PortDraft,
    registry: &[IoBinding],
) {
    let bindings = binding_options(registry);
    let selected = registry.iter().position(|b| b.id == draft.io);
    let endpoints = endpoint_options(selected.and_then(|i| registry.get(i)), draft.is_input);
    let selected_endpoint = endpoints.iter().position(|e| e.as_str() == draft.endpoint);

    port_window.set_binding_options(fresh_options(bindings));
    port_window.set_endpoint_options(fresh_options(endpoints));
    port_window.set_selected_binding_index(selected.map_or(-1, |i| i as i32));
    port_window.set_selected_endpoint_index(selected_endpoint.map_or(-1, |i| i as i32));
    port_window.set_is_input(draft.is_input);
    port_window.set_block_enabled(draft.enabled);
    port_window.set_show_endpoint_warning(false);
    *ctx.port_draft.borrow_mut() = Some(draft);
    let _ = port_window.show();
}

/// The binding registry of the live session (empty when there is no session).
pub(crate) fn session_registry(session: &Option<ProjectSession>) -> Vec<IoBinding> {
    session
        .as_ref()
        .map(|s| s.io_bindings.borrow().clone())
        .unwrap_or_default()
}

pub(crate) fn wire_port_window(
    window: &AppWindow,
    port_window: &ChainPortWindow,
    ctx: PortWiringCtx,
) {
    // --- pick a binding: refresh the endpoint list for the new binding ---
    {
        let port_draft = ctx.port_draft.clone();
        let project_session = ctx.project_session.clone();
        let weak = port_window.as_weak();
        port_window.on_select_binding(move |index: i32| {
            let Some(pw) = weak.upgrade() else { return };
            let registry = session_registry(&project_session.borrow());
            let Some(binding) = registry.get(index.max(0) as usize) else {
                return;
            };
            let mut draft_borrow = port_draft.borrow_mut();
            let Some(draft) = draft_borrow.as_mut() else {
                return;
            };
            draft.io = binding.id.clone();
            draft.endpoint.clear();
            let endpoints = endpoint_options(Some(binding), draft.is_input);
            let first = endpoints.first().cloned();
            pw.set_endpoint_options(fresh_options(endpoints));
            if let Some(first) = first {
                draft.endpoint = first.to_string();
                pw.set_selected_endpoint_index(0);
            } else {
                pw.set_selected_endpoint_index(-1);
            }
        });
    }

    // --- pick an endpoint ---
    {
        let port_draft = ctx.port_draft.clone();
        let weak = port_window.as_weak();
        port_window.on_select_endpoint(move |index: i32| {
            let Some(pw) = weak.upgrade() else { return };
            let mut draft_borrow = port_draft.borrow_mut();
            let Some(draft) = draft_borrow.as_mut() else {
                return;
            };
            if let Some(name) = pw.get_endpoint_options().row_data(index.max(0) as usize) {
                draft.endpoint = name.to_string();
            }
        });
    }

    // --- OK: persist the endpoint on the block ---
    {
        let ctx_save = ctx.clone_for_callback();
        let weak_window = window.as_weak();
        let weak_port = port_window.as_weak();
        port_window.on_save(move || {
            let Some(pw) = weak_port.upgrade() else {
                return;
            };
            let draft = ctx_save.port_draft.borrow().clone();
            let Some(draft) = draft else { return };
            if draft.io.is_empty() || draft.endpoint.is_empty() {
                // Nothing picked yet — the page shows its own warning.
                pw.set_show_endpoint_warning(true);
                return;
            }
            let mut session_borrow = ctx_save.project_session.borrow_mut();
            let Some(session) = session_borrow.as_mut() else {
                let _ = pw.hide();
                return;
            };
            let chain_id = {
                let proj = session.project.borrow();
                let Some(chain) = proj.chains.get(draft.chain_index) else {
                    let _ = pw.hide();
                    return;
                };
                chain.id.clone()
            };
            let command = if draft.is_input {
                Command::Chain(ChainCommand::SaveChainInputEndpoints {
                    chain: chain_id.clone(),
                    block_index: draft.block_index,
                    io: draft.io.clone(),
                    endpoint: draft.endpoint.clone(),
                })
            } else {
                Command::Chain(ChainCommand::SaveChainOutputEndpoints {
                    chain: chain_id.clone(),
                    block_index: draft.block_index,
                    io: draft.io.clone(),
                    endpoint: draft.endpoint.clone(),
                })
            };
            if let Err(e) = session.dispatcher.dispatch(command) {
                log::error!("port save error: {e}");
                pw.set_show_endpoint_warning(true);
                return;
            }
            // #127: the endpoint change is a GRAPH change, so the chain has to
            // be rebuilt — asked for on the bus, never by calling the
            // controller from here. Scoped to this chain by identity.
            if let Err(e) = request_chain_sync(session, &chain_id) {
                log::error!("port save runtime sync error: {e}");
            }
            replace_project_chains(
                &ctx_save.project_chains,
                &session.project.borrow(),
                &ctx_save.input_chain_devices.borrow(),
                &ctx_save.output_chain_devices.borrow(),
                &[],
            );
            if let Some(window) = weak_window.upgrade() {
                sync_project_dirty(
                    &window,
                    session,
                    &ctx_save.saved_project_snapshot,
                    &ctx_save.project_dirty,
                    ctx_save.auto_save,
                );
            }
            *ctx_save.port_draft.borrow_mut() = None;
            let _ = pw.hide();
        });
    }

    // --- toggle enabled ---
    {
        let ctx_toggle = ctx.clone_for_callback();
        let weak_port = port_window.as_weak();
        port_window.on_toggle_enabled(move || {
            let Some(pw) = weak_port.upgrade() else {
                return;
            };
            let draft = ctx_toggle.port_draft.borrow().clone();
            let Some(draft) = draft else { return };
            let mut session_borrow = ctx_toggle.project_session.borrow_mut();
            let Some(session) = session_borrow.as_mut() else {
                return;
            };
            let (chain_id, block_id) = {
                let proj = session.project.borrow();
                let Some(chain) = proj.chains.get(draft.chain_index) else {
                    return;
                };
                let Some(block) = chain.blocks.get(draft.block_index) else {
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
                log::error!("port toggle error: {e}");
                return;
            }
            // #85/#127/#881: a PORT is not a processor — `runtime_block_builders`
            // skips Input/Output as routing metadata and `runtime_segments`
            // splits the chain on the enabled ones. The rebuild that re-splits
            // the chain is asked for by the DISPATCHER (a routing toggle takes
            // `sync_chain`, not the in-place fade), so every transport gets it
            // and this callback must not ask for a second one.
            replace_project_chains(
                &ctx_toggle.project_chains,
                &session.project.borrow(),
                &ctx_toggle.input_chain_devices.borrow(),
                &ctx_toggle.output_chain_devices.borrow(),
                &[],
            );
            let enabled = session
                .project
                .borrow()
                .chains
                .get(draft.chain_index)
                .and_then(|c| c.blocks.get(draft.block_index))
                .map(|b| b.enabled)
                .unwrap_or(true);
            pw.set_block_enabled(enabled);
            if let Some(d) = ctx_toggle.port_draft.borrow_mut().as_mut() {
                d.enabled = enabled;
            }
        });
    }

    // --- delete the block ---
    {
        let ctx_del = ctx.clone_for_callback();
        let weak_port = port_window.as_weak();
        port_window.on_delete_block(move || {
            let Some(pw) = weak_port.upgrade() else {
                return;
            };
            let draft = ctx_del.port_draft.borrow().clone();
            let Some(draft) = draft else { return };
            let mut session_borrow = ctx_del.project_session.borrow_mut();
            let Some(session) = session_borrow.as_mut() else {
                return;
            };
            let (chain_id, block_id) = {
                let proj = session.project.borrow();
                let Some(chain) = proj.chains.get(draft.chain_index) else {
                    return;
                };
                let Some(block) = chain.blocks.get(draft.block_index) else {
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
                log::error!("port delete error: {e}");
                return;
            }
            // #127: `RemoveBlock` changes the graph and applies no runtime
            // effect of its own, so the rebuild is requested on the bus — the
            // same road `block_delete_wiring` takes.
            let _ = request_chain_sync(session, &chain_id);
            replace_project_chains(
                &ctx_del.project_chains,
                &session.project.borrow(),
                &ctx_del.input_chain_devices.borrow(),
                &ctx_del.output_chain_devices.borrow(),
                &[],
            );
            *ctx_del.port_draft.borrow_mut() = None;
            let _ = pw.hide();
        });
    }

    // --- cancel ---
    {
        let port_draft = ctx.port_draft.clone();
        let weak_port = port_window.as_weak();
        port_window.on_cancel(move || {
            *port_draft.borrow_mut() = None;
            if let Some(pw) = weak_port.upgrade() {
                let _ = pw.hide();
            }
        });
    }
}

impl PortWiringCtx {
    fn clone_for_callback(&self) -> Self {
        Self {
            port_draft: self.port_draft.clone(),
            project_session: self.project_session.clone(),
            project_chains: self.project_chains.clone(),
            saved_project_snapshot: self.saved_project_snapshot.clone(),
            project_dirty: self.project_dirty.clone(),
            input_chain_devices: self.input_chain_devices.clone(),
            output_chain_devices: self.output_chain_devices.clone(),
            auto_save: self.auto_save,
        }
    }
}

#[cfg(test)]
#[path = "port_wiring_tests.rs"]
mod tests;
