//! Chain CRUD handler (file-per-feature; #436 dispatcher split).
//! Behaviour byte-identical to the original inline arm — pure move.

use anyhow::Result;

use crate::chain_validation;
use crate::command::Command;
use crate::event::Event;
use crate::local_dispatcher::LocalDispatcher;

impl LocalDispatcher {
    /// Chain CRUD commands: add/configure/remove/volume.
    pub(crate) fn handle_chain_crud(&self, cmd: Command) -> Result<Vec<Event>> {
        match cmd {
            // ── Chain CRUD ────────────────────────────────────────────────────
            Command::AddChain { chain } => {
                // Validate that enabling this chain (enabled=true) would not
                // conflict with existing enabled chains.
                // Note: new chains start with enabled=false, so no conflict
                // is possible. We still run the check in case the caller
                // sets enabled=true on the supplied chain.
                if chain.enabled {
                    let proj = self.project.borrow();
                    chain_validation::validate_no_channel_conflict(&proj, &chain, None)
                        .map_err(|e| anyhow::anyhow!("{}", e))?;
                }
                let chain_id = chain.id.clone();
                self.project.borrow_mut().chains.push(chain);
                Ok(vec![
                    Event::ChainAdded { chain: chain_id },
                    Event::ProjectMutated,
                ])
            }
            Command::ConfigureChain { chain } => {
                let chain_id = chain.id.clone();
                self.with_chain(&chain_id, |existing| {
                    // Preserve runtime-only state (enabled) — callers must use
                    // ToggleChainEnabled to change the running state.
                    let keep_enabled = existing.enabled;
                    *existing = chain;
                    existing.enabled = keep_enabled;
                    Ok(())
                })?;
                Ok(vec![
                    Event::ChainConfigured { chain: chain_id },
                    Event::ProjectMutated,
                ])
            }
            Command::RemoveChain { chain } => {
                {
                    let mut proj = self.project.borrow_mut();
                    let pre_len = proj.chains.len();
                    proj.chains.retain(|c| c.id != chain);
                    if proj.chains.len() == pre_len {
                        return Err(anyhow::anyhow!("chain not found: {:?}", chain));
                    }
                }
                // #436: a rig chain (`rig:<input>`) must also drop its
                // RigInput, else any re-projection resurrects it. This
                // used to be done by hand in the GUI — now it's here.
                if let (Some(rig), Some(name)) =
                    (self.rig.borrow().clone(), chain.0.strip_prefix("rig:"))
                {
                    rig.borrow_mut().remove_input(name);
                }
                Ok(vec![Event::ChainRemoved { chain }, Event::ProjectMutated])
            }
            // ── Chain volume (issue #440) ─────────────────────────────────────
            Command::SetChainVolume { chain, value } => {
                self.with_chain(&chain, |c| {
                    c.volume = value;
                    Ok(())
                })?;
                Ok(vec![
                    Event::ChainVolumeChanged { chain, value },
                    Event::ProjectMutated,
                ])
            }
            other => unreachable!("handle_chain_crud received non-crud command: {other:?}"),
        }
    }
}
