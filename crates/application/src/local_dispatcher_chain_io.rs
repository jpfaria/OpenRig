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
                self.with_chain(&chain, |c| {
                    c.blocks = preset_blocks;
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
