//! #791 — `ToneDoctorCommand::DiagnoseChainTone` / `ApplyToneDoctorFix`.
//!
//! The doctor's signal is the chain's own: the DI loop when one is loaded,
//! otherwise whatever live input the adapter registered via
//! [`LocalDispatcher::attach_tone_doctor_input`]. Picking the source here — not
//! in a frontend — is what makes the verdict identical for GUI, MCP and gRPC.
//!
//! The diagnosis re-renders the chain once per block (and reloads NAM captures
//! from disk), so it runs on its own task and lands through
//! `poll_async_results` as `Event::ChainToneDiagnosed`, mirroring the DI decode
//! (#693). The apply is cheap and synchronous: it replays the measured fix as
//! ordinary block-parameter commands, so every observer sees the same events it
//! would from a knob turn.

use anyhow::{anyhow, Result};

use engine::tone_profile_table::ProfileTable;

use crate::command::{Command, ToneDoctorCommand};
use crate::dispatcher::CommandDispatcher;
use crate::event::Event;
use crate::local_dispatcher::{AsyncDone, LocalDispatcher};
use crate::tone_doctor_report::{diagnose, fix_commands, ToneRun, DIAGNOSE_BLOCK};

/// How much signal to analyse when the caller does not say.
const DEFAULT_ANALYZE_SECONDS: u32 = 5;

impl LocalDispatcher {
    pub(crate) fn handle_tone_doctor(&self, cmd: Command) -> Result<Vec<Event>> {
        let Command::ToneDoctor(cmd) = cmd else {
            unreachable!("handle_tone_doctor received a non-tone-doctor command: {cmd:?}");
        };
        match cmd {
            ToneDoctorCommand::DiagnoseChainTone {
                chain,
                genre,
                seconds,
            } => {
                let chain_def = self
                    .chain_snapshot(&chain)
                    .ok_or_else(|| anyhow!("chain not found: {}", chain.0))?;
                let seconds = seconds.unwrap_or(DEFAULT_ANALYZE_SECONDS).max(1) as usize;
                let limits = ProfileTable::embedded().limits_for(genre.as_deref());

                // The DI is the reproducible source and wins when present; the
                // live tap is what the player hears when it is not.
                let capture = match self.di_loop_for_chain(&chain) {
                    Some(di) => {
                        let capture: crate::local_dispatcher::ToneDoctorCapture =
                            Box::new(move || Some((di.stereo_frames(), di.src_sr() as f32)));
                        Some(capture)
                    }
                    None => self
                        .tone_doctor_input
                        .borrow()
                        .as_ref()
                        .and_then(|provider| provider(&chain, seconds)),
                };
                let Some(capture) = capture else {
                    return Err(anyhow!(
                        "no signal to analyse for chain '{}': load a DI loop or enable the chain",
                        chain.0
                    ));
                };

                // Accepted: a reader must be able to tell "working on it" from
                // "nothing ever happened" — the tool call itself returns no
                // events, so this is the only signal a non-event transport has.
                {
                    let mut runs = self.tone_doctor_runs.borrow_mut();
                    let previous = runs.get(&chain).and_then(|run| run.tone.clone());
                    runs.insert(chain.clone(), ToneRun::running(previous));
                }

                let tx = self.async_done_tx.clone();
                std::thread::Builder::new()
                    .name("tone-doctor".into())
                    .spawn(move || {
                        let result = match capture() {
                            Some((mut input, sample_rate)) => {
                                let cap = seconds * sample_rate as usize;
                                if input.len() > cap {
                                    input.truncate(cap);
                                }
                                diagnose(&chain_def, &input, sample_rate, DIAGNOSE_BLOCK, &limits)
                            }
                            None => Err("no signal captured".to_string()),
                        };
                        let _ = tx.send(AsyncDone::ToneDiagnosis(chain, result));
                    })
                    .map_err(|e| anyhow!("failed to spawn tone-doctor task: {e}"))?;

                Ok(vec![])
            }

            ToneDoctorCommand::ApplyToneDoctorFix { chain } => {
                let report = self
                    .tone_doctor_runs
                    .borrow()
                    .get(&chain)
                    .and_then(|run| run.tone.clone())
                    .ok_or_else(|| {
                        anyhow!(
                            "no diagnosis for chain '{}': run DiagnoseChainTone first",
                            chain.0
                        )
                    })?;
                let fix = report.suggestion.ok_or_else(|| {
                    anyhow!(
                        "the last diagnosis of chain '{}' found nothing to fix",
                        chain.0
                    )
                })?;

                let mut events = Vec::new();
                for cmd in fix_commands(&chain, &fix) {
                    events.extend(self.dispatch(cmd)?);
                }
                events.push(Event::ChainToneFixApplied {
                    chain,
                    block: fix.block,
                    path: fix.param_path,
                    value: fix.suggested,
                });
                Ok(events)
            }
        }
    }
}

#[cfg(test)]
#[path = "local_dispatcher_tone_doctor_tests.rs"]
mod tests;
