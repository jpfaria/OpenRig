//! #323 — the per-chain looper commands (file-per-feature; #436 split).
//!
//! The dispatcher owns the PROJECT side of a looper: which loopers a chain
//! has, and their persisted parameters.
//!
//! Transport actions carry no project state at all (a recording is runtime
//! state), so they are pure events: the command exists so every transport —
//! GUI button, MIDI footswitch, MCP tool — travels the same bus.
//!
//! #127: the RUNTIME side is applied here too, through
//! [`crate::runtime_control::RuntimeControl`]. It used to be applied by the
//! GUI callback that had just dispatched (`looper_callbacks::dispatch_and_apply`),
//! with the external-event drain running a second, parallel copy — so a looper
//! driven from anywhere else recorded nothing, and two roads led to one
//! runtime. Every handler below takes the chain AFTER its project half ran,
//! so the frontend reconciles the loop's isolated stream against what is true
//! now. Nothing here touches the audio thread: the doors flip control-plane
//! state the callback reads lock-free.

use anyhow::{anyhow, Result};

use project::chain::{Chain, LooperConfig, LOOPER_MAX_PER_CHAIN};

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
                let uid = {
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
                    // uid 0 marks a free slot on the audio thread, so ids start
                    // at 1 and never reuse a live one.
                    let uid = c.loopers.iter().map(|l| l.uid).max().unwrap_or(0) + 1;
                    c.loopers.push(LooperConfig::new(uid));
                    uid
                };
                // #808: this is the one looper door allowed to wake audio. The
                // panel arms REC only against a live store, so a looper added
                // with nothing running would be a looper the user cannot
                // record into.
                if let Some((control, chain_def)) = self.looper_control(&chain) {
                    control.create_looper(&chain_def, uid)?;
                }
                Ok(vec![Event::ChainLooperAdded { chain, looper: uid }])
            }

            LooperCommand::RemoveChainLooper { chain, looper } => {
                {
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
                }
                // Delete stops the sound: the chain the door is handed no
                // longer lists this looper, so its armed stream is stale and
                // the reconcile silences it at once.
                if let Some((control, chain_def)) = self.looper_control(&chain) {
                    control.remove_looper(&chain_def, looper);
                }
                Ok(vec![Event::ChainLooperRemoved { chain, looper }])
            }

            LooperCommand::SetChainLooperTransport {
                chain,
                looper,
                action,
            } => {
                let looper = self.resolve_looper(&chain, looper)?;
                // The whole action travels, `PlayStop` included: only the store
                // knows whether that one button means play or stop, so
                // resolving it in a frontend would give each transport its own
                // answer.
                if let Some((control, chain_def)) = self.looper_control(&chain) {
                    control.looper_transport(&chain_def, looper, action)?;
                }
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
                if let Some((control, chain_def)) = self.looper_control(&chain) {
                    control.set_looper_param(&chain_def, looper, param);
                }
                Ok(vec![Event::ChainLooperParamChanged {
                    chain,
                    looper,
                    param,
                }])
            }

            // #826 — the waveform edits. The whole change is runtime state (the
            // recorded audio), so these carry no project half at all: the door
            // reshapes the loop and the sidecar wav is rewritten on the next
            // project save, the one path that owns the project's files.
            LooperCommand::EditChainLooperAudio {
                chain,
                looper,
                edit,
            } => {
                let looper = self.resolve_looper(&chain, looper)?;
                if let Some((control, chain_def)) = self.looper_control(&chain) {
                    control.apply_looper_edit(&chain_def, looper, edit)?;
                }
                Ok(vec![])
            }

            LooperCommand::UndoChainLooperEdit { chain, looper } => {
                let looper = self.resolve_looper(&chain, looper)?;
                if let Some((control, chain_def)) = self.looper_control(&chain) {
                    control.undo_looper_edit(&chain_def, looper);
                }
                Ok(vec![])
            }

            LooperCommand::RedoChainLooperEdit { chain, looper } => {
                let looper = self.resolve_looper(&chain, looper)?;
                if let Some((control, chain_def)) = self.looper_control(&chain) {
                    control.redo_looper_edit(&chain_def, looper);
                }
                Ok(vec![])
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
                self.with_looper(&chain, looper, |cfg| cfg.input = input.clone())?;
                // Tell the store to record from the chosen input next time REC
                // starts (it drops the current record tap so it re-subscribes).
                if let Some((control, chain_def)) = self.looper_control(&chain) {
                    control.set_looper_input(&chain_def, looper, input);
                }
                Ok(vec![Event::ChainLooperInputChanged { chain, looper }])
            }

            LooperCommand::SetChainLooperOutput {
                chain,
                looper,
                output,
            } => {
                self.with_looper(&chain, looper, |cfg| cfg.output = output.clone())?;
                if let Some((control, chain_def)) = self.looper_control(&chain) {
                    control.set_looper_output(&chain_def, looper, output);
                }
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

    /// #323/#127: write every recorded loop beside the project and point each
    /// looper's config at the file, right before the project is serialized.
    ///
    /// A loop is audio, so it never goes into the project YAML: each non-empty
    /// looper becomes a wav under `<project>.loops/` and the chain remembers
    /// only the name. The export used to run in the GUI's Save callback,
    /// immediately before it dispatched `SaveProject` — so a save issued from
    /// anywhere else wrote a project pointing at audio nobody had written.
    ///
    /// The PCM crosses as an `Arc` handle carrying its own recorded rate, so
    /// nothing is copied and no sidecar can be stamped with another stream's
    /// rate. With no store hosted (the rig is stopped) NOTHING is touched:
    /// "nothing to export" is not "nothing was ever recorded", and treating
    /// the two alike would erase the loops of an earlier session.
    pub(crate) fn export_project_loops(&self, project_path: &std::path::Path) {
        let Some(control) = self.runtime_control() else {
            return;
        };
        // Snapshot the chains and END the borrow here: the frontend's control
        // reads the project back while it exports.
        let chains: Vec<Chain> = self.project.borrow().chains.clone();
        for chain in chains {
            if chain.loopers.is_empty() {
                continue;
            }
            let Some(exported) = control.export_chain_loops(&chain) else {
                continue;
            };
            let files: Vec<(u64, Option<String>)> = chain
                .loopers
                .iter()
                .filter_map(|cfg| {
                    let Some((_, pcm)) = exported.iter().find(|(uid, _)| *uid == cfg.uid) else {
                        // Nothing recorded: forget any stale pointer.
                        return Some((cfg.uid, None));
                    };
                    match crate::looper_audio::write_loop_wav(
                        project_path,
                        &chain.id,
                        cfg.uid,
                        pcm.samples(),
                        pcm.sample_rate(),
                    ) {
                        Ok(name) => Some((cfg.uid, Some(name))),
                        Err(err) => {
                            // A sidecar that could not be written must never
                            // clear the pointer to the one that already exists.
                            log::error!("saving loop {} of chain {}: {err}", cfg.uid, chain.id.0);
                            None
                        }
                    }
                })
                .collect();
            let mut proj = self.project.borrow_mut();
            let Some(c) = proj.chains.iter_mut().find(|c| c.id == chain.id) else {
                continue;
            };
            for (uid, file) in files {
                if let Some(cfg) = c.loopers.iter_mut().find(|l| l.uid == uid) {
                    cfg.audio_file = file;
                }
            }
        }
    }

    /// The attached runtime control and the chain as it stands NOW, or `None`
    /// when this process hosts no audio runtime (an MCP-only transport, a
    /// unit test) or the chain is gone.
    ///
    /// Both come out together on purpose: the project borrow ends here,
    /// BEFORE the frontend is called, because a `RuntimeControl` reads the
    /// project back while it reconciles (the same rule the DI's `chain_def`
    /// snapshot follows).
    fn looper_control(
        &self,
        chain: &domain::ids::ChainId,
    ) -> Option<(
        std::rc::Rc<dyn crate::runtime_control::RuntimeControl>,
        Chain,
    )> {
        Some((self.runtime_control()?, self.chain_def(chain)?))
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

#[cfg(test)]
#[path = "local_dispatcher_looper_runtime_tests.rs"]
mod runtime_tests;

#[cfg(test)]
#[path = "local_dispatcher_looper_save_tests.rs"]
mod save_tests;
