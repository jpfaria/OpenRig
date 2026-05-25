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
                        if let Some(input_name) =
                            crate::local_dispatcher_chain_crud::add_chain_to_rig(
                                &mut rig.borrow_mut(),
                                &chain,
                            )
                        {
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
                input_blocks,
            } => {
                self.with_chain(&chain, |c| {
                    // Remove all existing Input blocks, retaining non-input blocks.
                    c.blocks
                        .retain(|b| !matches!(&b.kind, project::block::AudioBlockKind::Input(_)));
                    // Insert the new input blocks at the head (inputs-first convention).
                    for (i, blk) in input_blocks.into_iter().enumerate() {
                        c.blocks.insert(i, blk);
                    }
                    Ok(())
                })?;
                Ok(vec![
                    Event::ChainInputEndpointsSaved {
                        chain: chain.clone(),
                    },
                    Event::ProjectMutated,
                ])
            }

            Command::SaveChainOutputEndpoints {
                chain,
                output_blocks,
            } => {
                let cloned_outputs = output_blocks.clone();
                self.with_chain(&chain, |c| {
                    // Remove all existing Output blocks, retaining non-output blocks.
                    c.blocks
                        .retain(|b| !matches!(&b.kind, project::block::AudioBlockKind::Output(_)));
                    // Append the new output blocks at the tail (outputs-last convention).
                    c.blocks.extend(output_blocks);
                    Ok(())
                })?;
                // Persist the edit into the rig so the next rig→legacy
                // projection (reload, scene/preset switch, runtime resync)
                // sees the same output. Without this the user's edit lives
                // only in the in-memory legacy chain and disappears on the
                // next projection — "I fix the output and it never persists".
                if let Some(input_name) = chain.0.strip_prefix("rig:") {
                    if let Some(rig) = self.rig.borrow().clone() {
                        propagate_outputs_to_rig(
                            &mut rig.borrow_mut(),
                            input_name,
                            &cloned_outputs,
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

/// Write the chain's user-edited outputs into the rig under stable per-input
/// keys, and point `input.routing` at them so `rig_to_chains` re-emits the
/// same Output block on every projection. Replaces every previous output
/// owned by this input (key prefix `<input_name>:`) — the user's latest
/// `SaveChainOutputEndpoints` is the new truth. No-op if the rig has no
/// such input.
pub(crate) fn propagate_outputs_to_rig(
    rig: &mut project::rig::RigProject,
    input_name: &str,
    output_blocks: &[project::block::AudioBlock],
) {
    let owned_prefix = format!("{input_name}:");
    rig.outputs.retain(|k, _| !k.starts_with(&owned_prefix));
    let Some(input) = rig.inputs.get_mut(input_name) else {
        return;
    };
    input.routing.clear();
    let mut idx = 0usize;
    for block in output_blocks {
        let project::block::AudioBlockKind::Output(ob) = &block.kind else {
            continue;
        };
        for entry in &ob.entries {
            let key = format!("{input_name}:{idx}");
            rig.outputs.insert(
                key.clone(),
                project::rig::RigOutput {
                    label: None,
                    entry: entry.clone(),
                },
            );
            input.routing.push(key);
            idx += 1;
        }
    }
}
