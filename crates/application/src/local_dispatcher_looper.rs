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

use crate::command::{Command, LooperCommand, LooperParam};
use crate::event::Event;
use crate::local_dispatcher::LocalDispatcher;

impl LocalDispatcher {
    /// Handle every `LooperCommand`.
    pub(crate) fn handle_looper(&self, cmd: Command) -> Result<Vec<Event>> {
        let Command::Looper(cmd) = cmd else {
            unreachable!("handle_looper received a non-looper command: {cmd:?}");
        };
        match cmd {
            LooperCommand::AddChainLooper { chain } => {
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
                Ok(vec![Event::ChainLooperAdded { chain, looper: uid }])
            }

            LooperCommand::RemoveChainLooper { chain, looper } => {
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

            LooperCommand::SetChainLooperTransport {
                chain,
                looper,
                action,
            } => {
                let looper = self.resolve_looper(&chain, looper)?;
                Ok(vec![Event::ChainLooperTransportChanged {
                    chain,
                    looper,
                    action,
                }])
            }

            LooperCommand::SetChainLooperParam {
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

            LooperCommand::SetChainLooperAudioFile {
                chain,
                looper,
                file,
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
                    cfg.audio_file = file;
                }
                Ok(vec![Event::ChainLooperAudioFileChanged { chain, looper }])
            }

            LooperCommand::SetChainLooperInput {
                chain,
                looper,
                input,
            } => {
                self.with_looper(&chain, looper, |cfg| cfg.input = input)?;
                Ok(vec![Event::ChainLooperInputChanged { chain, looper }])
            }

            LooperCommand::SetChainLooperOutput {
                chain,
                looper,
                output,
            } => {
                self.with_looper(&chain, looper, |cfg| cfg.output = output)?;
                Ok(vec![Event::ChainLooperOutputChanged { chain, looper }])
            }

            LooperCommand::SetChainLooperPreset {
                chain,
                looper,
                preset,
            } => {
                self.with_looper(&chain, looper, |cfg| cfg.preset = preset)?;
                Ok(vec![Event::ChainLooperPresetChanged { chain, looper }])
            }

        }
    }

    /// Locate a looper's config and mutate it, or fail loudly.
    fn with_looper(
        &self,
        chain: &domain::ids::ChainId,
        looper: u64,
        edit: impl FnOnce(&mut project::chain::LooperConfig),
    ) -> Result<()> {
        let mut proj = self.project.borrow_mut();
        let cfg = proj
            .chains
            .iter_mut()
            .find(|c| &c.id == chain)
            .ok_or_else(|| anyhow!("chain not found: {chain:?}"))?
            .loopers
            .iter_mut()
            .find(|l| l.uid == looper)
            .ok_or_else(|| anyhow!("looper not found: {looper}"))?;
        edit(cfg);
        Ok(())
    }

    /// Resolve the looper a transport action addresses.
    ///
    /// `0` is the footswitch sentinel: a pedal has no uid to send, so it
    /// means "this chain's first looper" (uid 0 is never assigned). Any other
    /// value must exist — a transport action for something that does not
    /// exist is a caller bug, never a silent no-op.
    fn resolve_looper(&self, chain: &domain::ids::ChainId, looper: u64) -> Result<u64> {
        let proj = self.project.borrow();
        let c = proj
            .chains
            .iter()
            .find(|c| &c.id == chain)
            .ok_or_else(|| anyhow!("chain not found: {chain:?}"))?;
        if looper == 0 {
            return c
                .loopers
                .first()
                .map(|l| l.uid)
                .ok_or_else(|| anyhow!("chain has no looper to control"));
        }
        if c.loopers.iter().any(|l| l.uid == looper) {
            Ok(looper)
        } else {
            Err(anyhow!("looper not found: {looper}"))
        }
    }
}

#[cfg(test)]
#[path = "local_dispatcher_looper_tests.rs"]
mod tests;
