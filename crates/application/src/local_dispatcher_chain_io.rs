//! Chain I/O replace + preset-load handler (file-per-feature; #436 split).
//! Behaviour byte-identical to the original inline arm — pure move.

use anyhow::Result;

use crate::command::Command;
use crate::event::Event;
use crate::local_dispatcher::LocalDispatcher;

impl LocalDispatcher {
    /// Chain I/O block replacement + preset load commands.
    pub(crate) fn handle_chain_io_replace(&self, cmd: Command) -> Result<Vec<Event>> {
        match cmd {
            Command::SaveChainIo {
                chain,
                input_block,
                output_block,
            } => {
                self.with_chain(&chain, |c| {
                    let in_pos = c
                        .blocks
                        .iter()
                        .position(|b| matches!(&b.kind, project::block::AudioBlockKind::Input(_)));
                    let Some(in_idx) = in_pos else {
                        return Err(anyhow::anyhow!(
                            "chain {:?} has no InputBlock to replace",
                            chain
                        ));
                    };
                    let out_pos = c
                        .blocks
                        .iter()
                        .position(|b| matches!(&b.kind, project::block::AudioBlockKind::Output(_)));
                    let Some(out_idx) = out_pos else {
                        return Err(anyhow::anyhow!(
                            "chain {:?} has no OutputBlock to replace",
                            chain
                        ));
                    };
                    c.blocks[in_idx] = input_block;
                    c.blocks[out_idx] = output_block;
                    Ok(())
                })?;
                Ok(vec![
                    Event::ChainIoSaved {
                        chain: chain.clone(),
                    },
                    Event::ProjectMutated,
                ])
            }

            // ── Chain presets ─────────────────────────────────────────────────
            Command::LoadChainPreset {
                chain,
                preset_blocks,
            } => {
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
