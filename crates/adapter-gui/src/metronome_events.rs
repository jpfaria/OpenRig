//! #14 — the metronome's dispatch door and the only place its events are
//! applied.
//!
//! A control never writes the session, the window, `config.yaml` or the audio
//! runtime: it dispatches a `Command` and this file applies whatever `Event`
//! comes back. So a tempo set from MCP or a MIDI footswitch lands on the same
//! three surfaces the GUI's own knob does, and the dispatcher's clamps and enum
//! validation are the only definition of what a legal value is.

use std::cell::RefCell;
use std::rc::Rc;
use std::time::Instant;

use application::app_config_persist::persist_metronome;
use application::command::{Command, MetronomeCommand};
use application::dispatcher::CommandDispatcher;
use application::event::Event;
use infra_cpal::AudioDeviceDescriptor;
use slint::SharedString;

use crate::metronome_session::MetronomeSession;
use crate::metronome_wiring::{start_click, stop_click, MetronomeCtx};
use crate::MetronomeWindow;

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
                if let Some(mw) = ctx.window.upgrade() {
                    mw.set_metronome_enabled(enabled);
                    if !enabled {
                        mw.set_current_beat(0);
                        mw.set_counting_in(false);
                    }
                }
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
                ctx.session
                    .borrow_mut()
                    .set_output_device(device_id.clone());
                let persisted = device_id.clone();
                persist_metronome(None, move |config| config.output_device = persisted);
                // A running click follows the new device immediately; a stopped
                // one simply opens there next time.
                if let Some(rt) = ctx.project_runtime.borrow().as_ref() {
                    if rt.metronome_active() {
                        if let Some(id) = device_id.as_deref() {
                            if let Err(e) = rt.start_metronome(id) {
                                log::warn!("[metronome] reopen on '{id}' failed: {e}");
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
    if let Some(mw) = ctx.window.upgrade() {
        render_settings(&mw, ctx);
    }
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

/// Mirror the whole session onto the window's properties. One function so no
/// control can forget a derived field (a time signature changes both the label
/// and the lamp count).
pub(crate) fn render_settings_from(
    window: &MetronomeWindow,
    session: &MetronomeSession,
    output_label: &str,
) {
    window.set_bpm(session.bpm());
    window.set_beats_per_bar(session.beats_per_bar() as i32);
    window.set_time_signature_index(session.time_signature_index());
    window.set_time_signature_label(SharedString::from(session.time_signature_label()));
    window.set_subdivision_index(session.subdivision_index());
    window.set_subdivision_label(SharedString::from(session.subdivision_label()));
    window.set_timbre_index(session.timbre_index());
    window.set_timbre_label(SharedString::from(session.timbre_label()));
    window.set_volume(session.volume());
    window.set_count_in(session.count_in());
    window.set_output_key(SharedString::from(session.output_device().unwrap_or("")));
    window.set_output_label(SharedString::from(output_label));
}

pub(crate) fn render_settings(window: &MetronomeWindow, ctx: &MetronomeCtx) {
    let session = ctx.session.borrow();
    let label = output_device_label(&ctx.devices, session.output_device());
    render_settings_from(window, &session, &label);
}

/// Display name of the picked device. Falls back to the raw id (a device that
/// is saved but currently unplugged still shows what it was), and to an empty
/// label when nothing is picked — the select then renders its placeholder.
fn output_device_label(
    cache: &Rc<RefCell<Vec<AudioDeviceDescriptor>>>,
    device_id: Option<&str>,
) -> String {
    let Some(device_id) = device_id else {
        return String::new();
    };
    cache
        .borrow()
        .iter()
        .find(|d| d.id == device_id)
        .map(|d| d.name.clone())
        .unwrap_or_else(|| device_id.to_string())
}
