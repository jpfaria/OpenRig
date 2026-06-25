//! Chain save/endpoints handler (file-per-feature; #436 dispatcher split).
//! Behaviour byte-identical to the original inline arm — pure move.

use anyhow::Result;

use crate::command::Command;
use crate::event::Event;
use crate::local_dispatcher::LocalDispatcher;

impl LocalDispatcher {
    /// Chain save/upsert + input/output endpoint replacement commands.
    pub(crate) fn handle_chain_save(&self, cmd: Command) -> Result<Vec<Event>> {
        match cmd {
            // ── Chain save (upsert) ───────────────────────────────────────────
            Command::SaveChain { mut chain } => {
                // Detect upsert vs. create *before* mutating the project.
                let is_create = !self
                    .project
                    .borrow()
                    .chains
                    .iter()
                    .any(|c| c.id == chain.id);
                if is_create {
                    if let Some(rig) = self.rig.borrow().clone() {
                        // Capture into a separate statement so the scrutinee's
                        // `borrow_mut()` is released before the body re-borrows.
                        let created = {
                            let mut rig_mut = rig.borrow_mut();
                            crate::local_dispatcher_chain_crud::add_chain_to_rig(
                                &mut rig_mut,
                                &chain,
                            )
                        };
                        if let Some(input_name) = created {
                            // #716: carry the editor's I/O binding selection
                            // onto the freshly-created rig input so it survives
                            // reopen.
                            if let Some(input) =
                                rig.borrow_mut().inputs.get_mut(&input_name)
                            {
                                input.io_binding_ids = chain.io_binding_ids.clone();
                            }
                            // Re-tag the chain id so the chains-screen
                            // preset/scene combobox can find this chain
                            // in the rig — `rig_nav_rows` only recognises
                            // chains whose id starts with `rig:`.
                            chain.id = domain::ids::ChainId(format!("rig:{input_name}"));
                        }
                    }
                } else if let Some(rig) = self.rig.borrow().clone() {
                    // Upsert path: the chain already lives in the project
                    // and in the rig. The legacy `Project` re-projection
                    // takes its `description` from `RigInput.label`, so a
                    // rename here that only touches `chain.description`
                    // is wiped on the next `rig_to_chains` pass. Mirror
                    // the renamed description into the rig too.
                    if let Some(input_name) = chain.id.0.strip_prefix("rig:") {
                        let mut rig_mut = rig.borrow_mut();
                        if let Some(input) = rig_mut.inputs.get_mut(input_name) {
                            input.label = chain.description.clone();
                            // #716: persist the editor's I/O binding selection
                            // into the rig so it survives reopen (the GUI saves
                            // via SaveChain, not SetChainIoBindings).
                            input.io_binding_ids = chain.io_binding_ids.clone();
                        }
                    }
                }
                let chain_id = chain.id.clone();
                let mut proj = self.project.borrow_mut();
                if let Some(existing) = proj.chains.iter_mut().find(|c| c.id == chain_id) {
                    let keep_enabled = existing.enabled;
                    *existing = chain;
                    existing.enabled = keep_enabled;
                } else {
                    proj.chains.push(chain);
                }
                Ok(vec![
                    Event::ChainSaved { chain: chain_id },
                    Event::ProjectMutated,
                ])
            }

            // ── Chain I/O endpoints ───────────────────────────────────────────
            Command::SaveChainInputEndpoints {
                chain,
                block_index,
                io,
                endpoint,
            } => {
                self.with_chain(&chain, |c| {
                    let blk = c.blocks.get_mut(block_index).ok_or_else(|| {
                        anyhow::anyhow!(
                            "SaveChainInputEndpoints: chain {:?} has no block at index {}",
                            chain,
                            block_index
                        )
                    })?;
                    match &mut blk.kind {
                        project::block::AudioBlockKind::Input(ib) => {
                            ib.io = io.clone();
                            ib.endpoint = endpoint.clone();
                            Ok(())
                        }
                        _ => Err(anyhow::anyhow!(
                            "SaveChainInputEndpoints: block at index {} in chain {:?} \
                             is not an InputBlock",
                            block_index,
                            chain
                        )),
                    }
                })?;
                // Propagate the binding reference into the rig so the next
                // rig→legacy projection keeps the user's io/endpoint.
                if let Some(input_name) = chain.0.strip_prefix("rig:") {
                    if let Some(rig) = self.rig.borrow().clone() {
                        if let Some(rig_input) =
                            rig.borrow_mut().inputs.get_mut(input_name)
                        {
                            rig_input.io = io;
                            rig_input.endpoint = endpoint;
                        }
                    }
                }
                Ok(vec![
                    Event::ChainInputEndpointsSaved {
                        chain: chain.clone(),
                    },
                    Event::ProjectMutated,
                ])
            }

            Command::SaveChainOutputEndpoints {
                chain,
                block_index,
                io,
                endpoint,
            } => {
                self.with_chain(&chain, |c| {
                    let blk = c.blocks.get_mut(block_index).ok_or_else(|| {
                        anyhow::anyhow!(
                            "SaveChainOutputEndpoints: chain {:?} has no block at index {}",
                            chain,
                            block_index
                        )
                    })?;
                    match &mut blk.kind {
                        project::block::AudioBlockKind::Output(ob) => {
                            ob.io = io.clone();
                            ob.endpoint = endpoint.clone();
                            Ok(())
                        }
                        _ => Err(anyhow::anyhow!(
                            "SaveChainOutputEndpoints: block at index {} in chain {:?} \
                             is not an OutputBlock",
                            block_index,
                            chain
                        )),
                    }
                })?;
                // Propagate the binding reference into the rig so the next
                // rig→legacy projection sees the updated output block.
                if let Some(input_name) = chain.0.strip_prefix("rig:") {
                    if let Some(rig) = self.rig.borrow().clone() {
                        propagate_output_ref_to_rig(
                            &mut rig.borrow_mut(),
                            input_name,
                            block_index,
                            &io,
                            &endpoint,
                        );
                    }
                }
                Ok(vec![
                    Event::ChainOutputEndpointsSaved {
                        chain: chain.clone(),
                    },
                    Event::ProjectMutated,
                ])
            }
            other => unreachable!("handle_chain_save received non-save command: {other:?}"),
        }
    }
}

/// Store the binding reference set by `SaveChainOutputEndpoints` /
/// `SaveChainIo` into the rig's output at a stable per-block key so the next
/// `rig_to_legacy_project` projection re-emits a block with the same
/// `io`/`endpoint` values.
///
/// The key is `"{input_name}:{block_index}"`. `block_index` is the position
/// of the output block inside the chain's `blocks` vec — stable across
/// projections as long as blocks are not reordered.
///
/// No-op when the rig has no input named `input_name`.
pub(crate) fn propagate_output_ref_to_rig(
    rig: &mut project::rig::RigProject,
    input_name: &str,
    block_index: usize,
    io: &str,
    endpoint: &str,
) {
    let key = format!("{input_name}:{block_index}");
    let Some(input) = rig.inputs.get_mut(input_name) else {
        return;
    };
    // Ensure the key is in the routing list (idempotent).
    if !input.routing.contains(&key) {
        input.routing.push(key.clone());
    }
    // Store or update the binding reference in rig.outputs under the stable key.
    // The `RigOutput.io` and `.endpoint` fields are propagated to the output block
    // by `rig_to_chains` during projection (#716).
    let entry = rig.outputs.entry(key).or_insert_with(|| project::rig::RigOutput {
        label: None,
        io: String::new(),
        endpoint: String::new(),
    });
    entry.io = io.to_string();
    entry.endpoint = endpoint.to_string();
}
