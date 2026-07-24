//! Chain I/O replace + preset-load handler (file-per-feature; #436 split).
//! Behaviour byte-identical to the original inline arm — pure move.

use anyhow::Result;

use crate::command::{ChainCommand, Command};
use crate::event::Event;
use crate::local_dispatcher::LocalDispatcher;

impl LocalDispatcher {
    /// Chain I/O block replacement + preset load commands.
    pub(crate) fn handle_chain_io_replace(&self, cmd: Command) -> Result<Vec<Event>> {
        match cmd {
            Command::Chain(ChainCommand::SaveChainIo {
                chain,
                input_block_index,
                output_block_index,
                io,
                endpoint,
            }) => {
                self.with_chain(&chain, |c| {
                    // Set io/endpoint on the input block.
                    let in_blk = c.blocks.get_mut(input_block_index).ok_or_else(|| {
                        anyhow::anyhow!(
                            "SaveChainIo: chain {:?} has no block at input_block_index {}",
                            chain,
                            input_block_index
                        )
                    })?;
                    match &mut in_blk.kind {
                        project::block::AudioBlockKind::Input(ib) => {
                            ib.io = io.clone();
                            ib.endpoint = endpoint.clone();
                        }
                        _ => {
                            return Err(anyhow::anyhow!(
                                "SaveChainIo: block at input_block_index {} in chain {:?} \
                                 is not an InputBlock",
                                input_block_index,
                                chain
                            ))
                        }
                    }
                    // Set io/endpoint on the output block.
                    let out_blk = c.blocks.get_mut(output_block_index).ok_or_else(|| {
                        anyhow::anyhow!(
                            "SaveChainIo: chain {:?} has no block at output_block_index {}",
                            chain,
                            output_block_index
                        )
                    })?;
                    match &mut out_blk.kind {
                        project::block::AudioBlockKind::Output(ob) => {
                            ob.io = io.clone();
                            ob.endpoint = endpoint.clone();
                        }
                        _ => {
                            return Err(anyhow::anyhow!(
                                "SaveChainIo: block at output_block_index {} in chain {:?} \
                                 is not an OutputBlock",
                                output_block_index,
                                chain
                            ))
                        }
                    }
                    Ok(())
                })?;
                // Propagate binding reference into the rig for the output block.
                if let Some(input_name) = chain.0.strip_prefix("rig:") {
                    if let Some(rig) = self.rig.borrow().clone() {
                        crate::local_dispatcher_chain_save::propagate_output_ref_to_rig(
                            &mut rig.borrow_mut(),
                            input_name,
                            output_block_index,
                            &io,
                            &endpoint,
                        );
                    }
                }
                Ok(vec![
                    Event::ChainIoSaved {
                        chain: chain.clone(),
                    },
                    Event::ProjectMutated,
                ])
            }

            // ── Chain presets ─────────────────────────────────────────────────
            Command::Chain(ChainCommand::LoadChainPreset {
                chain,
                preset_instrument,
                preset_blocks,
            }) => {
                // Guard: reject the load if the preset's instrument tag differs
                // from the target chain's instrument (#627). This is the hard
                // gate that applies regardless of transport (GUI / MCP / gRPC).
                // Untagged legacy presets default to "electric_guitar" (same as
                // the serde default on ChainBlocksPreset.instrument).
                {
                    let project = self.project.borrow();
                    let chain_instrument = project
                        .chains
                        .iter()
                        .find(|c| c.id == chain)
                        .map(|c| c.instrument.as_str())
                        .ok_or_else(|| {
                            anyhow::anyhow!("Command::LoadChainPreset: chain {:?} not found", chain)
                        })?;
                    if preset_instrument != chain_instrument {
                        anyhow::bail!(
                            "preset is for {preset_instrument}, chain is {chain_instrument}: \
                             cannot load a {preset_instrument} preset into a \
                             {chain_instrument} chain"
                        );
                    }
                }
                // Preset files are intentionally I/O-stripped (the adapter
                // parses the file and drops the I/O blocks before
                // dispatching, since I/O routing is per-machine). Preserve
                // the chain's existing I/O endpoints across the swap so
                // loading a preset doesn't leave the chain without an
                // output sink (which would fail validation with
                // "chain '...' has no output blocks").
                self.with_chain(&chain, |c| {
                    let mut inputs: Vec<project::block::AudioBlock> = Vec::new();
                    let mut outputs: Vec<project::block::AudioBlock> = Vec::new();
                    for b in &c.blocks {
                        match &b.kind {
                            project::block::AudioBlockKind::Input(_) => inputs.push(b.clone()),
                            project::block::AudioBlockKind::Output(_) => outputs.push(b.clone()),
                            _ => {}
                        }
                    }
                    let mut merged: Vec<project::block::AudioBlock> =
                        Vec::with_capacity(inputs.len() + preset_blocks.len() + outputs.len());
                    merged.extend(inputs);
                    merged.extend(preset_blocks);
                    merged.extend(outputs);
                    c.blocks = merged;
                    Ok(())
                })?;
                Ok(vec![Event::ChainPresetLoaded { chain }])
            }
            other => {
                unreachable!("handle_chain_io_replace received non-replace command: {other:?}")
            }
        }
    }
}
