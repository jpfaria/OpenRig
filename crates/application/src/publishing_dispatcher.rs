//! Responsibility: publishes the events an inner dispatcher produced.
//! `CommandDispatcher` decorator: dispatches via an inner `LocalDispatcher`,
//! then fans every resulting event batch out to an [`EventSink`]. This is the
//! single point where every frontend's command path becomes observable by
//! transports (MCP notifications). GUI- and MCP-originated commands both flow
//! through the one dispatcher a frontend holds, so wrapping it here captures
//! every state change with no per-call-site instrumentation.

use std::cell::RefCell;
use std::path::PathBuf;
use std::rc::Rc;
use std::sync::{Arc, RwLock};

use anyhow::Result;

use domain::ids::ChainId;
use domain::io_binding::IoBinding;
use engine::DiPcm;
use project::rig::RigProject;

use crate::bridge::EventSink;
use crate::command::Command;
use crate::di_loader::DiLoopSource;
use crate::dispatcher::CommandDispatcher;
use crate::event::Event;
use crate::local_dispatcher::{LocalDispatcher, ToneDoctorInput};
use crate::metronome_state::{MetronomeControlState, MetronomeSnapshot};
use crate::runtime_control::RuntimeControl;
use crate::selection_state::SelectionState;

pub struct PublishingDispatcher {
    inner: LocalDispatcher,
    sink: EventSink,
}

impl PublishingDispatcher {
    pub fn new(inner: LocalDispatcher, sink: EventSink) -> Self {
        Self { inner, sink }
    }

    /// #791: the wrapped dispatcher, for the reads a frontend serves from
    /// dispatcher-owned state (the Tone Doctor's last verdict).
    pub fn inner(&self) -> &LocalDispatcher {
        &self.inner
    }
}

impl CommandDispatcher for PublishingDispatcher {
    fn dispatch(&self, cmd: Command) -> Result<Vec<Event>> {
        // #693 diagnostic: name any command that holds the dispatching
        // (frontend) thread past one frame, so real-session stalls are
        // self-reporting instead of guessed at.
        let label = {
            let full = format!("{cmd:?}");
            full.split([' ', '{']).next().unwrap_or("?").to_string()
        };
        let t0 = std::time::Instant::now();
        let events = self.inner.dispatch(cmd)?;
        let elapsed = t0.elapsed();
        if elapsed > std::time::Duration::from_millis(50) {
            log::warn!("[ui-stall] Command::{label} held the dispatching thread for {elapsed:?}");
        }
        self.sink.publish(&events);
        // #693: refresh the read snapshot after every state change so
        // transports serve reads API-style on their own thread instead
        // of hopping to this (frontend) one.
        self.inner.publish_state_snapshot();
        Ok(events)
    }

    /// #693: async completions flow through the same fan-out a
    /// synchronous dispatch gets — transports see them as events.
    fn poll_async_results(&self) -> Vec<Event> {
        let events = self.inner.poll_async_results();
        if !events.is_empty() {
            self.sink.publish(&events);
            self.inner.publish_state_snapshot();
        }
        events
    }

    // #127: the wrapper is transparent for reads/attach too — every one of
    // these forwards to the wrapped `LocalDispatcher`, so a caller holding
    // this behind `&dyn CommandDispatcher` sees exactly what `.inner()`
    // already gave direct callers (adapter-console, adapter-mcp).

    fn selection_state(&self) -> Arc<RwLock<SelectionState>> {
        self.inner.selection_state()
    }

    fn engine_sr(&self) -> u32 {
        self.inner.engine_sr()
    }

    fn chain_snapshot(&self, chain: &ChainId) -> Option<project::chain::Chain> {
        self.inner.chain_snapshot(chain)
    }

    fn di_loop_for_chain(&self, chain: &ChainId) -> Option<Arc<DiPcm>> {
        self.inner.di_loop_for_chain(chain)
    }

    fn di_loop_source_for_chain(&self, chain: &ChainId) -> Option<DiLoopSource> {
        self.inner.di_loop_source_for_chain(chain)
    }

    fn tone_report_json(&self, chain: &ChainId) -> String {
        self.inner.tone_report_json(chain)
    }

    fn attach_rig(&self, rig: Rc<RefCell<RigProject>>) {
        self.inner.attach_rig(rig)
    }

    fn attach_presets_path(&self, path: PathBuf) {
        self.inner.attach_presets_path(path)
    }

    fn attach_project_path(&self, path: PathBuf) {
        self.inner.attach_project_path(path)
    }

    fn attach_config_path(&self, path: Option<PathBuf>) {
        self.inner.attach_config_path(path)
    }

    fn attach_engine_sr(&self, sr: u32) -> Vec<ChainId> {
        self.inner.attach_engine_sr(sr)
    }

    fn attach_tone_doctor_input(&self, provider: ToneDoctorInput) {
        self.inner.attach_tone_doctor_input(provider)
    }

    fn attach_runtime_control(&self, control: Rc<dyn RuntimeControl>) {
        self.inner.attach_runtime_control(control)
    }

    fn attach_io_bindings(&self, registry: Rc<RefCell<Vec<IoBinding>>>) {
        self.inner.attach_io_bindings(registry)
    }

    fn attach_metronome_state(&self, state: Rc<RefCell<MetronomeControlState>>) {
        self.inner.attach_metronome_state(state)
    }

    fn metronome_snapshot(&self) -> MetronomeSnapshot {
        self.inner.metronome_snapshot()
    }
}

#[cfg(test)]
#[path = "publishing_dispatcher_tests.rs"]
mod tests;
