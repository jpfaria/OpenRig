//! #14 — the metronome's dispatch door and the only place its events are
//! applied.
//!
//! A control never writes the session, the window, `config.yaml` or the audio
//! runtime: it dispatches a `Command` and this file applies whatever `Event`
//! comes back. So a tempo set from MCP or a MIDI footswitch lands on the same
//! three surfaces the GUI's own knob does, and the dispatcher's clamps and enum
//! validation are the only definition of what a legal value is.

use std::time::Instant;

use application::app_config_persist::persist_metronome;
use application::command::{Command, MetronomeCommand};
use application::dispatcher::CommandDispatcher;
use application::event::Event;
use slint::SharedString;

use crate::metronome_controls_wiring::refresh_metronome_outputs;
use crate::metronome_session::{resolve_output_endpoint, MetronomeSession};
use crate::metronome_wiring::{set_power_display, start_click, stop_click, MetronomeCtx};
use crate::MetronomeBridge;

/// Dispatch a metronome command and apply the events it produced.
pub(crate) fn dispatch(ctx: &MetronomeCtx, cmd: Command) {
    let events = {
        let borrowed = ctx.project_session.borrow();
        let Some(session) = borrowed.as_ref() else {
            return;
        };
        match session.dispatcher.dispatch(cmd) {
            Ok(events) => events,
            Err(e) => {
                log::warn!("[metronome] dispatch failed: {e}");
                return;
            }
        }
    };
    apply_events(ctx, events);
}

pub(crate) fn apply_events(ctx: &MetronomeCtx, events: Vec<Event>) {
    for event in events {
        match event {
            Event::MetronomeEnabledChanged { enabled } => {
                if enabled {
                    start_click(ctx);
                } else {
                    stop_click(ctx);
                }
                set_power_display(ctx, enabled);
            }
            Event::MetronomeBpmChanged { bpm } => {
                ctx.session.borrow_mut().set_bpm(bpm);
                persist_metronome(None, move |config| config.bpm = bpm);
                push_settings(ctx);
            }
            Event::MetronomeTimeSignatureChanged { beats_per_bar } => {
                ctx.session.borrow_mut().set_beats_per_bar(beats_per_bar);
                persist_metronome(None, move |config| config.beats_per_bar = beats_per_bar);
                push_settings(ctx);
            }
            Event::MetronomeSubdivisionChanged { subdivision } => {
                ctx.session.borrow_mut().set_subdivision_key(&subdivision);
                persist_metronome(None, move |config| config.subdivision = subdivision);
                push_settings(ctx);
            }
            Event::MetronomeTimbreChanged { timbre } => {
                ctx.session.borrow_mut().set_timbre_key(&timbre);
                persist_metronome(None, move |config| config.timbre = timbre);
                push_settings(ctx);
            }
            Event::MetronomeVolumeChanged { volume } => {
                ctx.session.borrow_mut().set_volume(volume);
                persist_metronome(None, move |config| config.volume = volume);
                push_settings(ctx);
            }
            Event::MetronomeCountInChanged { enabled } => {
                ctx.session.borrow_mut().set_count_in(enabled);
                persist_metronome(None, move |config| config.count_in = enabled);
                push_settings(ctx);
            }
            Event::MetronomeOutputChanged { device_id } => {
                // `device_id` now carries the chosen output-endpoint key (#14).
                ctx.session
                    .borrow_mut()
                    .set_output_device(device_id.clone());
                let persisted = device_id.clone();
                persist_metronome(None, move |config| config.output_device = persisted);
                // A running click follows the new endpoint immediately; a stopped
                // one simply opens there next time.
                if let Some(rt) = ctx.project_runtime.borrow().as_ref() {
                    if rt.metronome_active() {
                        let outputs =
                            refresh_metronome_outputs(&ctx.project_session, &ctx.outputs);
                        if let Some(out) =
                            resolve_output_endpoint(device_id.as_deref(), &outputs)
                        {
                            if let Err(e) = rt.start_metronome(&out.device_id, &out.channels) {
                                log::warn!("[metronome] reopen on '{}' failed: {e}", out.label);
                            }
                        }
                    }
                }
            }
            Event::MetronomeTapped => {
                let bpm = ctx.session.borrow_mut().tap_at(Instant::now());
                // The tap history is the adapter's; the tempo it implies still
                // travels as a Command so every observer sees the new tempo.
                if let Some(bpm) = bpm {
                    dispatch(
                        ctx,
                        Command::Metronome(MetronomeCommand::SetMetronomeBpm { bpm }),
                    );
                }
            }
            _ => {}
        }
    }
    render_settings(ctx);
}

/// Hand the current settings to the audio side. Cheap and idempotent: the
/// shared cell bumps a generation counter and the callback only re-reads when
/// it changed.
fn push_settings(ctx: &MetronomeCtx) {
    let settings = ctx.session.borrow().settings();
    if let Some(rt) = ctx.project_runtime.borrow().as_ref() {
        rt.set_metronome_settings(settings);
    }
}

/// Mirror the whole session onto one surface's bridge. One function so no
/// control can forget a derived field (a time signature changes both the label
/// and the lamp count).
pub(crate) fn render_settings_from(
    bridge: &MetronomeBridge,
    session: &MetronomeSession,
    output_key: &str,
    output_label: &str,
) {
    bridge.set_bpm(session.bpm());
    bridge.set_beats_per_bar(session.beats_per_bar() as i32);
    bridge.set_time_signature_index(session.time_signature_index());
    bridge.set_time_signature_label(SharedString::from(session.time_signature_label()));
    bridge.set_subdivision_index(session.subdivision_index());
    bridge.set_subdivision_label(SharedString::from(session.subdivision_label()));
    bridge.set_timbre_index(session.timbre_index());
    bridge.set_timbre_label(SharedString::from(session.timbre_label()));
    bridge.set_volume(session.volume());
    bridge.set_count_in(session.count_in());
    bridge.set_output_key(SharedString::from(output_key));
    bridge.set_output_label(SharedString::from(output_label));
}

/// Push the whole session onto every live surface's bridge.
pub(crate) fn render_settings(ctx: &MetronomeCtx) {
    // Re-read the project's endpoints so the field shows the resolved label even
    // on the very first open, before the picker has been touched.
    let outputs = refresh_metronome_outputs(&ctx.project_session, &ctx.outputs);
    let session = ctx.session.borrow();
    let resolved = resolve_output_endpoint(session.output_device(), &outputs);
    let key = resolved.as_ref().map(|o| o.key.clone()).unwrap_or_default();
    let label = resolved.map(|o| o.label).unwrap_or_default();
    ctx.for_each_bridge(|bridge| render_settings_from(bridge, &session, &key, &label));
}
