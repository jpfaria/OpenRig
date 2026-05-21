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
            Command::SaveChain { chain } => {
                let chain_id = chain.id.clone();
                // Detect upsert vs. create *before* mutating the project,
                // so we know whether to mirror the chain into the rig.
                let is_create = !self
                    .project
                    .borrow()
                    .chains
                    .iter()
                    .any(|c| c.id == chain_id);
                if is_create {
                    if let Some(rig) = self.rig.borrow().clone() {
                        crate::local_dispatcher_chain_crud::add_chain_to_rig(
                            &mut rig.borrow_mut(),
                            &chain,
                        );
                    }
                }
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
                self.with_chain(&chain, |c| {
                    // Remove all existing Output blocks, retaining non-output blocks.
                    c.blocks
                        .retain(|b| !matches!(&b.kind, project::block::AudioBlockKind::Output(_)));
                    // Append the new output blocks at the tail (outputs-last convention).
                    c.blocks.extend(output_blocks);
                    Ok(())
                })?;
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
