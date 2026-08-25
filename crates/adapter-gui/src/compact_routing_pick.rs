//! #881 — re-pointing a routing block (insert / mid port) from the compact view.
//!
//! In the row, a processing block picks its MODEL where a routing block picks
//! its E/S: same widget, same slot. The pick therefore arrives on the model
//! callback carrying a BINDING id, and this is where it becomes the command the
//! block actually persists — an insert saves its loop binding, a port saves the
//! binding plus that binding's first endpoint in its own direction.

use std::cell::RefCell;
use std::rc::Rc;

use application::command::{BlockCommand, ChainCommand, Command};
use project::block::AudioBlockKind;

use crate::runtime_sync_policy::request_chain_sync;
use crate::state::ProjectSession;

/// Whether the block at `(chain_index, block_index)` is routing, and if so
/// dispatch the binding change. Returns `true` when it handled the pick, so the
/// caller can skip the model path entirely.
pub(crate) fn dispatch_binding_pick(
    project_session: &Rc<RefCell<Option<ProjectSession>>>,
    chain_index: usize,
    block_index: usize,
    binding_id: &str,
) -> bool {
    let mut session_borrow = project_session.borrow_mut();
    let Some(session) = session_borrow.as_mut() else {
        return false;
    };
    let (chain_id, block_id, kind) = {
        let project = session.project.borrow();
        let Some(chain) = project.chains.get(chain_index) else {
            return false;
        };
        let Some(block) = chain.blocks.get(block_index) else {
            return false;
        };
        if !block.kind.is_routing() {
            return false;
        }
        (chain.id.clone(), block.id.clone(), block.kind.clone())
    };

    // A port also needs an endpoint; take the binding's first one on its own
    // side, the same seed the port editor uses when the E/S changes.
    let endpoint_of = |is_input: bool| {
        session
            .io_bindings
            .borrow()
            .iter()
            .find(|b| b.id == binding_id)
            .and_then(|b| {
                if is_input {
                    b.inputs.first()
                } else {
                    b.outputs.first()
                }
            })
            .map(|e| e.name.clone())
            .unwrap_or_default()
    };

    let command = match kind {
        AudioBlockKind::Insert(_) => Command::Block(BlockCommand::SaveInsertBlock {
            chain: chain_id.clone(),
            block: block_id,
            io: binding_id.to_string(),
        }),
        AudioBlockKind::Input(_) => Command::Chain(ChainCommand::SaveChainInputEndpoints {
            chain: chain_id.clone(),
            block_index,
            io: binding_id.to_string(),
            endpoint: endpoint_of(true),
        }),
        AudioBlockKind::Output(_) => Command::Chain(ChainCommand::SaveChainOutputEndpoints {
            chain: chain_id.clone(),
            block_index,
            io: binding_id.to_string(),
            endpoint: endpoint_of(false),
        }),
        _ => return false,
    };

    if let Err(e) = session.dispatcher.dispatch(command) {
        log::error!("[compact] re-pointing the routing block failed: {e}");
        return true;
    }
    // Routing is topology: the chain's streams have to follow the new E/S.
    if let Err(e) = request_chain_sync(session, &chain_id) {
        log::error!("[compact] runtime sync after re-pointing: {e}");
    }
    true
}
