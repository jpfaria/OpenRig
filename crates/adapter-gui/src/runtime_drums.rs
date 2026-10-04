//! Responsibility: opens the drum machine's own output stream for the GUI.
//!
//! The drums are an independent pipeline: their own cpal stream on the
//! endpoint the user picked, summed with the chains by the backend. Like the
//! metronome, only a start may create the audio runtime, so the drums play
//! with no chain enabled; every other door only follows what is open.
//!
//! A kit is decoded at the stream's sample rate on a worker thread and handed
//! to the callback through the shared cell, so neither the control thread nor
//! the audio thread ever waits on the disk.

use std::cell::RefCell;
use std::path::{Path, PathBuf};
use std::sync::Arc;

use anyhow::{bail, Result};

use application::drums_runtime::{DrumsRuntime, DrumsSetup};
use engine::drum_state::{DrumSettings, DrumsCell, Groove};

use crate::metronome_view::{output_endpoints, resolve_output_endpoint, ProjectOutput};
use crate::runtime_lifecycle::GuiRuntimeControl;
use crate::runtime_pipelines::ensure_runtime;
use crate::state::ProjectSession;

/// The kit the user chose, and the last load asked of the current stream.
#[derive(Default)]
pub(crate) struct DrumKitMemory {
    wanted: RefCell<Option<PathBuf>>,
    requested: RefCell<Option<(PathBuf, u32)>>,
}

impl DrumsRuntime for GuiRuntimeControl {
    fn start_drums(&self, setup: DrumsSetup<'_>) -> Result<()> {
        let Some(session) = self.session.session() else {
            return Ok(());
        };
        let Some(target) = drums_endpoint(&session, setup.output_key) else {
            bail!("no project output endpoint to play the drums through");
        };
        ensure_runtime(&self.runtime, &self.analyzers, &session)?;
        *self.drum_kit.wanted.borrow_mut() = setup.kit_dir.map(Path::to_path_buf);
        let borrow = self.runtime.borrow();
        let Some(controller) = borrow.as_ref() else {
            return Ok(());
        };
        let shared = controller.drums_shared();
        shared.set_settings(setup.settings);
        if let Some(groove) = setup.groove {
            shared.grooves().send(groove);
        }
        let rate = controller.start_drums(&target.device_id, &target.channels)?;
        shared.set_enabled(true);
        self.follow_kit(&shared, rate);
        Ok(())
    }

    fn stop_drums(&self) {
        if let Some(controller) = self.runtime.borrow().as_ref() {
            let shared = controller.drums_shared();
            shared.set_playing(false);
            shared.set_enabled(false);
            controller.stop_drums();
        }
    }

    fn set_drums_settings(&self, settings: DrumSettings) {
        self.with_shared(|shared| shared.set_settings(settings));
    }

    fn set_drums_playing(&self, playing: bool) {
        self.with_shared(|shared| shared.set_playing(playing));
    }

    fn trigger_drum_fill(&self) {
        self.with_shared(|shared| shared.request_fill());
    }

    fn set_drum_kit(&self, dir: &Path) {
        *self.drum_kit.wanted.borrow_mut() = Some(dir.to_path_buf());
        if let Some(controller) = self.runtime.borrow().as_ref() {
            if let Some(rate) = controller.drums_sample_rate() {
                self.follow_kit(&controller.drums_shared(), rate);
            }
        }
    }

    fn set_drum_groove(&self, groove: Arc<Groove>) {
        self.with_shared(|shared| shared.grooves().send(groove));
    }

    /// A key that resolves to nothing leaves the sounding drums alone.
    fn refresh_drums_output(&self, output_key: Option<&str>) -> Result<()> {
        let Some(session) = self.session.session() else {
            return Ok(());
        };
        let target = drums_endpoint(&session, output_key);
        let borrow = self.runtime.borrow();
        let Some(controller) = borrow.as_ref() else {
            return Ok(());
        };
        if !controller.drums_active() {
            return Ok(());
        }
        let Some(target) = target else {
            return Ok(());
        };
        let rate = controller.start_drums(&target.device_id, &target.channels)?;
        self.follow_kit(&controller.drums_shared(), rate);
        Ok(())
    }
}

impl GuiRuntimeControl {
    fn with_shared(&self, apply: impl FnOnce(&DrumsCell)) {
        if let Some(controller) = self.runtime.borrow().as_ref() {
            apply(&controller.drums_shared());
        }
    }

    /// Load the wanted kit at `rate` unless the cell already holds it.
    fn follow_kit(&self, shared: &DrumsCell, rate: u32) {
        let Some(dir) = self.drum_kit.wanted.borrow().clone() else {
            return;
        };
        let loaded = shared
            .kits()
            .latest()
            .is_some_and(|kit| kit.sample_rate() == rate);
        let requested = self.drum_kit.requested.borrow().clone();
        if loaded && requested.as_ref() == Some(&(dir.clone(), rate)) {
            return;
        }
        *self.drum_kit.requested.borrow_mut() = Some((dir.clone(), rate));
        spawn_kit_load(Arc::clone(shared), dir, rate);
    }
}

/// Decode the kit off the control thread; a load overtaken by a newer one is
/// dropped by its ticket.
fn spawn_kit_load(shared: DrumsCell, dir: PathBuf, rate: u32) {
    let ticket = shared.begin_kit_load();
    let spawned = std::thread::Builder::new()
        .name("drum-kit-load".into())
        .spawn(move || match application::drums::load_kit(&dir, rate) {
            Ok(kit) => {
                shared.finish_kit_load(ticket, Arc::new(kit));
            }
            Err(e) => log::warn!("drum kit {} failed to load: {e}", dir.display()),
        });
    if let Err(e) = spawned {
        log::warn!("drum kit loader thread failed to start: {e}");
    }
}

/// The endpoint the drums play through: the saved one while it exists,
/// otherwise the first the drums offer.
fn drums_endpoint(session: &ProjectSession, output_key: Option<&str>) -> Option<ProjectOutput> {
    let bindings = session.io_bindings.borrow();
    resolve_output_endpoint(output_key, &output_endpoints(&bindings, &[]))
}

#[cfg(test)]
#[path = "runtime_drums_tests.rs"]
mod tests;
