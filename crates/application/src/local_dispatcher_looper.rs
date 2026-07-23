//! #323 — the per-chain looper commands (file-per-feature; #436 split).
//!
//! The dispatcher owns the PROJECT side of a looper: which loopers a chain
//! has, and their persisted parameters. It never touches the audio thread —
//! the adapter wiring reacts to the emitted events and pushes the matching
//! `engine::LooperOp` onto the chain's runtimes (the #614 rule: a dispatch
//! alone is dead).
//!
//! Transport actions carry no project state at all (a recording is runtime
//! state), so they are pure events: the command exists so every transport —
//! GUI button, MIDI footswitch, MCP tool — travels the same bus.

use anyhow::{anyhow, Result};

use project::chain::{LooperConfig, LOOPER_MAX_PER_CHAIN};

use crate::command::{Command, LooperParam};
use crate::event::Event;
use crate::local_dispatcher::LocalDispatcher;

impl LocalDispatcher {
    /// Handle every `*ChainLooper*` command.
    pub(crate) fn handle_looper(&self, cmd: Command) -> Result<Vec<Event>> {
        match cmd {
            Command::AddChainLooper { chain } => {
                let mut proj = self.project.borrow_mut();
                let c = proj
                    .chains
                    .iter_mut()
                    .find(|c| c.id == chain)
                    .ok_or_else(|| anyhow!("chain not found: {chain:?}"))?;
                if c.loopers.len() >= LOOPER_MAX_PER_CHAIN {
                    return Err(anyhow!(
                        "chain already holds the maximum of {LOOPER_MAX_PER_CHAIN} loopers"
                    ));
                }
                // uid 0 marks a free slot on the audio thread, so ids start at
                // 1 and never reuse a live one.
                let uid = c.loopers.iter().map(|l| l.uid).max().unwrap_or(0) + 1;
                c.loopers.push(LooperConfig::new(uid));
                Ok(vec![Event::ChainLooperAdded {
                    chain,
                    looper: uid,
                }])
            }

            Command::RemoveChainLooper { chain, looper } => {
                let mut proj = self.project.borrow_mut();
                let c = proj
                    .chains
                    .iter_mut()
                    .find(|c| c.id == chain)
                    .ok_or_else(|| anyhow!("chain not found: {chain:?}"))?;
                let before = c.loopers.len();
                c.loopers.retain(|l| l.uid != looper);
                if c.loopers.len() == before {
                    return Err(anyhow!("looper not found: {looper}"));
                }
                Ok(vec![Event::ChainLooperRemoved { chain, looper }])
            }

            Command::SetChainLooperTransport {
                chain,
                looper,
                action,
            } => {
                self.require_looper(&chain, looper)?;
                Ok(vec![Event::ChainLooperTransportChanged {
                    chain,
                    looper,
                    action,
                }])
            }

            Command::SetChainLooperParam {
                chain,
                looper,
                param,
            } => {
                {
                    let mut proj = self.project.borrow_mut();
                    let cfg = proj
                        .chains
                        .iter_mut()
                        .find(|c| c.id == chain)
                        .ok_or_else(|| anyhow!("chain not found: {chain:?}"))?
                        .loopers
                        .iter_mut()
                        .find(|l| l.uid == looper)
                        .ok_or_else(|| anyhow!("looper not found: {looper}"))?;
                    match param {
                        LooperParam::Mix(v) => cfg.mix = v.clamp(0.0, 1.0),
                        LooperParam::Decay(v) => cfg.decay = v.clamp(0.0, 1.0),
                        LooperParam::Speed(s) => cfg.speed = s,
                        LooperParam::Reverse(v) => cfg.reverse = v,
                    }
                }
                Ok(vec![Event::ChainLooperParamChanged {
                    chain,
                    looper,
                    param,
                }])
            }

            other => unreachable!("handle_looper received a non-looper command: {other:?}"),
        }
    }

    /// Fail unless `chain` holds `looper` — a transport action for something
    /// that does not exist is a caller bug, never a silent no-op.
    fn require_looper(&self, chain: &domain::ids::ChainId, looper: u64) -> Result<()> {
        let proj = self.project.borrow();
        let c = proj
            .chains
            .iter()
            .find(|c| &c.id == chain)
            .ok_or_else(|| anyhow!("chain not found: {chain:?}"))?;
        if c.loopers.iter().any(|l| l.uid == looper) {
            Ok(())
        } else {
            Err(anyhow!("looper not found: {looper}"))
        }
    }
}

#[cfg(test)]
#[path = "local_dispatcher_looper_tests.rs"]
mod tests;
