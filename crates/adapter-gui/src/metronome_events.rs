//! Responsibility: dispatches a metronome command then mirrors the result.
//! #14 — the metronome's dispatch door, and the mirror it draws afterwards.
//!
//! A control never writes state, `config.yaml` or the audio runtime: it
//! dispatches a `Command` and this file re-renders whatever the dispatcher now
//! holds.
//!
//! #127 moved the substance out of here. Applying the events used to BE the
//! feature — this file started the click, pushed settings to the generator and
//! wrote the config — and it was reachable only from the GUI's own knob
//! callbacks, so a MIDI footswitch bound to `toggle_metronome` or an MCP client
//! flipped the mirror in `SelectionState` and never made a sound. All of that
//! is the dispatcher's now (`application::local_dispatcher_metronome`); what is
//! left here is what only a window can do: draw the knobs.

use application::command::Command;
use application::event::Event;
use engine::metronome_state::MetronomeSettings;
use slint::SharedString;

use crate::metronome_view::{
    resolve_output_endpoint, subdivision_index, subdivision_label, timbre_index, timbre_label,
    time_signature_index, time_signature_label,
};
use crate::metronome_wiring::{outputs, set_power_display, snapshot, MetronomeCtx};
use crate::MetronomeBridge;

/// Dispatch a metronome command and redraw from the state it produced.
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

/// Mirror the dispatcher's new state onto the panel.
///
/// Only POWER needs its own arm: it is the one control whose widget cannot be
/// derived from the settings. Everything else is redrawn wholesale, so no
/// event can be forgotten by a missing match arm.
pub(crate) fn apply_events(ctx: &MetronomeCtx, events: Vec<Event>) {
    for event in events {
        if let Event::MetronomeEnabledChanged { enabled } = event {
            set_power_display(ctx, enabled);
        }
    }
    render_settings(ctx);
}

/// Mirror the settings onto one surface's bridge. One function so no control
/// can forget a derived field (a time signature changes both the label and the
/// lamp count).
pub(crate) fn render_settings_from(
    bridge: &MetronomeBridge,
    settings: &MetronomeSettings,
    output_key: &str,
    output_label: &str,
) {
    bridge.set_bpm(settings.bpm);
    bridge.set_beats_per_bar(settings.beats_per_bar as i32);
    bridge.set_time_signature_index(time_signature_index(settings.beats_per_bar));
    bridge.set_time_signature_label(SharedString::from(time_signature_label(
        settings.beats_per_bar,
    )));
    bridge.set_subdivision_index(subdivision_index(settings.subdivision.key()));
    bridge.set_subdivision_label(SharedString::from(subdivision_label(
        settings.subdivision.key(),
    )));
    bridge.set_timbre_index(timbre_index(settings.timbre.key()));
    bridge.set_timbre_label(SharedString::from(timbre_label(settings.timbre.key())));
    bridge.set_volume(settings.volume);
    bridge.set_count_in(settings.count_in);
    bridge.set_output_key(SharedString::from(output_key));
    bridge.set_output_label(SharedString::from(output_label));
}

/// Push the dispatcher's whole snapshot onto every live surface's bridge.
///
/// No project open ⇒ no dispatcher and nothing to show; the panel is only
/// reachable from a project screen.
pub(crate) fn render_settings(ctx: &MetronomeCtx) {
    let Some(state) = snapshot(ctx) else {
        return;
    };
    // Re-read the project's endpoints so the field shows the resolved label even
    // on the very first open, before the picker has been touched.
    let endpoints = outputs(ctx);
    let resolved = resolve_output_endpoint(state.output_key.as_deref(), &endpoints);
    let key = resolved.as_ref().map(|o| o.key.clone()).unwrap_or_default();
    let label = resolved.map(|o| o.label).unwrap_or_default();
    ctx.for_each_bridge(|bridge| render_settings_from(bridge, &state.settings, &key, &label));
    // What the knobs now show, so the lamp timer can tell "changed elsewhere"
    // from "already drawn".
    *ctx.rendered.borrow_mut() = Some(state);
}
