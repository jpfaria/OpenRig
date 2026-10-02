//! Responsibility: implements the backing-track player's doors of the runtime seam.
//!
//! The player is an independent pipeline (invariant #4), so it follows the
//! same rules as the metronome's doors in `runtime_pipelines`: only a start may
//! wake audio, a follow never may, and a pause or stop is never an error.
//!
//! The track lives in the controller's player worker, and the controller is
//! recreated when the rig is stopped and started again. That is why the start
//! loads the track again before playing: the load is a no-op while the worker
//! still holds the same file, and brings it back when the controller is new.

use std::cell::RefCell;
use std::path::Path;
use std::rc::Rc;

use anyhow::{bail, Result};

use engine::player::settings::PlayerSettings;
use infra_cpal::ProjectRuntimeController;

use crate::metronome_view::{output_endpoints, resolve_output_endpoint, MetronomeOutput};
use crate::runtime_analyzers::AnalyzerSessions;
use crate::runtime_pipelines::ensure_runtime;
use crate::state::ProjectSession;

type Runtime = Rc<RefCell<Option<ProjectRuntimeController>>>;

/// Hands `track` to the running controller's worker. With the rig stopped
/// nothing is woken: the start loads it once audio exists.
pub(crate) fn load_player_track(runtime: &Runtime, track: &Path) {
    if let Some(controller) = runtime.borrow().as_ref() {
        controller.load_player_track(track, application::player_decode::decode_player_file);
    }
}

/// Plays `track` on the endpoint `output_key` names. The endpoint is resolved
/// first so a project with no output gets a reason, not a silent switch.
pub(crate) fn start_player(
    runtime: &Runtime,
    analyzers: &AnalyzerSessions,
    session: &ProjectSession,
    track: &Path,
    settings: PlayerSettings,
    output_key: Option<&str>,
) -> Result<()> {
    let Some(target) = player_endpoint(session, output_key) else {
        bail!("no project output endpoint to play the backing track through");
    };
    ensure_runtime(runtime, analyzers, session)?;
    let borrow = runtime.borrow();
    let Some(controller) = borrow.as_ref() else {
        return Ok(());
    };
    controller.load_player_track(track, application::player_decode::decode_player_file);
    controller.set_player_settings(settings);
    controller.start_player(&target.device_id, &target.channels)
}

pub(crate) fn pause_player(runtime: &Runtime) {
    if let Some(controller) = runtime.borrow().as_ref() {
        controller.pause_player();
    }
}

pub(crate) fn stop_player(runtime: &Runtime) {
    if let Some(controller) = runtime.borrow().as_ref() {
        controller.stop_player();
    }
}

pub(crate) fn seek_player(runtime: &Runtime, seconds: f64) {
    if let Some(controller) = runtime.borrow().as_ref() {
        controller.seek_player(seconds);
    }
}

pub(crate) fn push_player_settings(runtime: &Runtime, settings: PlayerSettings) {
    if let Some(controller) = runtime.borrow().as_ref() {
        controller.set_player_settings(settings);
    }
}

/// Moves an OPEN player to the endpoint `output_key` now names. A closed
/// player stays closed: picking an output is not playing.
pub(crate) fn refresh_player_output(
    runtime: &Runtime,
    session: &ProjectSession,
    output_key: Option<&str>,
) -> Result<()> {
    let target = player_endpoint(session, output_key);
    let borrow = runtime.borrow();
    let Some(controller) = borrow.as_ref() else {
        return Ok(());
    };
    if !controller.player_active() {
        return Ok(());
    }
    let Some(target) = target else {
        return Ok(());
    };
    controller.refresh_player_output(&target.device_id, &target.channels)
}

/// The saved endpoint while it exists, otherwise the project's first.
fn player_endpoint(session: &ProjectSession, output_key: Option<&str>) -> Option<MetronomeOutput> {
    let bindings = session.io_bindings.borrow();
    resolve_output_endpoint(output_key, &output_endpoints(&bindings))
}
