//! #127: the INDEPENDENT pipelines' runtime doors — the DI loop (#614/#771)
//! and the metronome click (#14).
//!
//! Both are invariant #4 applied literally: each opens its own stream, the
//! backend sums it, and neither may ever be mixed into, routed through or
//! grouped with a chain's runtime. That is also why they share a shape the
//! chain doors do not have:
//!
//! * a **start** (`arm_di`, `start_metronome`) is the only kind of door
//!   allowed to create the controller — the pipeline has to sound with NO
//!   chain enabled (#808);
//! * a **follow** (`refresh_di`, `refresh_metronome_output`) moves something
//!   that is ALREADY sounding and must leave a silent pipeline silent, so it
//!   never wakes audio up;
//! * a **stop** is idempotent and never an error: there is nothing to fail
//!   about silence.
//!
//! These are the bodies of `GuiRuntimeControl`'s doors, split out of
//! `runtime_lifecycle` (which owns the CHAIN runtime's lifecycle and was at
//! its line cap). They are part of the same seam implementation — the wiring
//! modules still reach all of this through a `Command`.

use std::rc::Rc;
use std::sync::Arc;

use anyhow::{bail, Result};

use domain::ids::ChainId;
use engine::metronome_state::{MetronomeSettings, MetronomeShared};
use engine::DiPcm;
use infra_cpal::ProjectRuntimeController;
use project::chain::Chain;

use crate::metronome_view::{output_endpoints, resolve_output_endpoint, MetronomeOutput};
use crate::runtime_lifecycle::ensure_runtime;
use crate::state::ProjectSession;

/// The app-wide runtime handle, as every door here takes it.
type Runtime = Rc<std::cell::RefCell<Option<ProjectRuntimeController>>>;

// ── DI loop ─────────────────────────────────────────────────────────────

/// #614/#771: play the loop on THIS chain's isolated DI stream — resolve the
/// chain's chosen output, render through a copy of its graph off-thread, park
/// on that output's cell. The guitar runtime is never touched (invariant #4).
///
/// #808: the runtime is a PRECONDITION of the arm, not a user action. The DI
/// has to play with no chain enabled, so a stopped rig gets its controller
/// here — the same lazy creation the chain-enable path does, minus the
/// `enabled` gate.
pub(crate) fn arm_di(
    runtime: &Runtime,
    session: &ProjectSession,
    chain: &Chain,
    pcm: Arc<DiPcm>,
) -> Result<()> {
    ensure_runtime(runtime, session)?;
    let borrow = runtime.borrow();
    let Some(controller) = borrow.as_ref() else {
        return Ok(());
    };
    controller.arm_di_stream(chain, pcm)
}

pub(crate) fn disarm_di(runtime: &Runtime, chain: &ChainId) {
    if let Some(controller) = runtime.borrow().as_ref() {
        controller.disarm_di_stream(chain);
    }
}

/// Follow a change the PLAYING loop depends on: the picked output (#771) or
/// the device rate it was resampled to (#669). A stream that is not sounding
/// stays silent — and unlike [`arm_di`] this never creates a runtime, because
/// nobody asked to hear anything.
pub(crate) fn refresh_di(runtime: &Runtime, chain: &Chain, pcm: Arc<DiPcm>) -> Result<()> {
    let borrow = runtime.borrow();
    let Some(controller) = borrow.as_ref() else {
        return Ok(());
    };
    if !controller.di_stream_active(&chain.id) {
        return Ok(());
    }
    controller.arm_di_stream(chain, pcm)
}

// ── metronome ───────────────────────────────────────────────────────────

/// #14/#127: open the click on the endpoint `output_key` names.
///
/// The endpoint is resolved FIRST, on purpose: with nothing to play through
/// there is no point waking the audio host, and the caller gets a reason
/// instead of a switch that looks on and makes no sound. Only once a target
/// exists does this become the metronome's [`arm_di`] — the one metronome door
/// allowed to create the controller (#808).
pub(crate) fn start_metronome(
    runtime: &Runtime,
    session: &ProjectSession,
    settings: MetronomeSettings,
    output_key: Option<&str>,
) -> Result<()> {
    let Some(target) = metronome_endpoint(session, output_key) else {
        bail!("no project output endpoint to play the metronome through");
    };
    ensure_runtime(runtime, session)?;
    let borrow = runtime.borrow();
    let Some(controller) = borrow.as_ref() else {
        return Ok(());
    };
    controller.set_metronome_settings(settings);
    mark_click_playing(&controller.metronome_shared(), || {
        controller.start_metronome(&target.device_id, &target.channels)
    })
}

/// Open the stream FIRST; mark the click playing only if it opened.
///
/// `enabled` is not a request, it is the answer every reader gives to "is
/// anything audible": the beat lamps' 33 ms timer, `gui_live_source`'s
/// `LiveSource::metronome`, and `openrig://metronome` — where the live flag
/// WINS over the dispatcher's snapshot. Setting it before the open leaves a
/// device that is gone or busy reading as a playing click forever: the
/// dispatcher records `running=false` on the `Err` and POWER goes dark, then
/// the lamp timer lights it again from this flag, beat frozen, no sound.
fn mark_click_playing(shared: &MetronomeShared, open: impl FnOnce() -> Result<()>) -> Result<()> {
    open()?;
    // Start from beat one of the bar instead of wherever a previous run left
    // the phase. Before `enabled`, so the first buffer that counts is beat one.
    shared.request_restart();
    shared.set_enabled(true);
    Ok(())
}

pub(crate) fn stop_metronome(runtime: &Runtime) {
    if let Some(controller) = runtime.borrow().as_ref() {
        controller.metronome_shared().set_enabled(false);
        controller.stop_metronome();
    }
}

/// Hand new settings to the generator's shared cell. It bumps a generation
/// counter the callback compares against, so a tempo edit is picked up on the
/// next buffer without reopening the stream — a live edit never drops audio,
/// and nothing is allocated or locked on the audio thread.
pub(crate) fn push_metronome_settings(runtime: &Runtime, settings: MetronomeSettings) {
    if let Some(controller) = runtime.borrow().as_ref() {
        controller.set_metronome_settings(settings);
    }
}

/// Move a PLAYING click to the endpoint `output_key` now names. A stopped
/// click stays stopped and no runtime is created — picking is not playing.
///
/// A key that resolves to nothing leaves the current stream alone: the pick
/// failing is no reason to cut a click that is already sounding.
pub(crate) fn refresh_metronome_output(
    runtime: &Runtime,
    session: &ProjectSession,
    output_key: Option<&str>,
) -> Result<()> {
    let target = metronome_endpoint(session, output_key);
    let borrow = runtime.borrow();
    let Some(controller) = borrow.as_ref() else {
        return Ok(());
    };
    if !controller.metronome_active() {
        return Ok(());
    }
    let Some(target) = target else {
        return Ok(());
    };
    controller.start_metronome(&target.device_id, &target.channels)
}

/// The output endpoint the click plays through: the saved one while it exists,
/// otherwise the project's first (#14). `None` only when the project has no
/// output endpoint at all.
///
/// The key stays opaque to `application`; this frontend owns the binding
/// registry, so it is the one that can turn it into a device and channels.
fn metronome_endpoint(
    session: &ProjectSession,
    output_key: Option<&str>,
) -> Option<MetronomeOutput> {
    let bindings = session.io_bindings.borrow();
    resolve_output_endpoint(output_key, &output_endpoints(&bindings))
}

#[cfg(test)]
#[path = "runtime_pipelines_tests.rs"]
mod tests;
