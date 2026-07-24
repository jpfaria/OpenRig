//! #14 — the metronome's control callbacks: the tempo row, the four knobs, the
//! count-in pill and the output-device select.
//!
//! Every one of them does the same two things: translate the widget's value
//! into a `Command` and dispatch it. Nothing here touches the session or the
//! audio runtime — that is `metronome_events`' job, driven by the events the
//! dispatch returns.

use std::cell::RefCell;
use std::rc::Rc;

use application::command::{Command, MetronomeCommand};
use infra_cpal::AudioDeviceDescriptor;
use slint::{ModelRc, SharedString, VecModel};

use crate::metronome_events::dispatch;
use crate::metronome_session::{subdivision_key, timbre_key, time_signature_beats};
use crate::metronome_wiring::MetronomeCtx;
use crate::{MetronomeWindow, SelectOption};

pub(crate) fn wire_controls(metronome_window: &MetronomeWindow, ctx: &MetronomeCtx) {
    {
        let ctx = ctx.clone_ctx();
        metronome_window.on_set_bpm(move |bpm| {
            dispatch(
                &ctx,
                Command::Metronome(MetronomeCommand::SetMetronomeBpm { bpm }),
            );
        });
    }
    {
        let ctx = ctx.clone_ctx();
        metronome_window.on_tap(move || {
            dispatch(&ctx, Command::Metronome(MetronomeCommand::MetronomeTap));
        });
    }
    {
        let ctx = ctx.clone_ctx();
        metronome_window.on_set_time_signature(move |index| {
            dispatch(
                &ctx,
                Command::Metronome(MetronomeCommand::SetMetronomeTimeSignature {
                    beats_per_bar: time_signature_beats(index),
                }),
            );
        });
    }
    {
        let ctx = ctx.clone_ctx();
        metronome_window.on_set_subdivision(move |index| {
            dispatch(
                &ctx,
                Command::Metronome(MetronomeCommand::SetMetronomeSubdivision {
                    subdivision: subdivision_key(index).to_string(),
                }),
            );
        });
    }
    {
        let ctx = ctx.clone_ctx();
        metronome_window.on_set_timbre(move |index| {
            dispatch(
                &ctx,
                Command::Metronome(MetronomeCommand::SetMetronomeTimbre {
                    timbre: timbre_key(index).to_string(),
                }),
            );
        });
    }
    {
        let ctx = ctx.clone_ctx();
        metronome_window.on_set_volume(move |volume| {
            dispatch(
                &ctx,
                Command::Metronome(MetronomeCommand::SetMetronomeVolume { volume }),
            );
        });
    }
    {
        let ctx = ctx.clone_ctx();
        metronome_window.on_set_count_in(move |enabled| {
            dispatch(
                &ctx,
                Command::Metronome(MetronomeCommand::SetMetronomeCountIn { enabled }),
            );
        });
    }
}

// ── output device select ────────────────────────────────────────────────

pub(crate) fn wire_output_select(metronome_window: &MetronomeWindow, ctx: &MetronomeCtx) {
    {
        let ctx = ctx.clone_ctx();
        metronome_window.on_output_opened(move || {
            // Re-enumerate on open so a device plugged in since the last look
            // shows up; the host list is cached, so this is cheap.
            output_device_ids(&ctx.devices);
            publish_output_options(&ctx, "");
        });
    }
    {
        let ctx = ctx.clone_ctx();
        metronome_window.on_output_query(move |query| {
            publish_output_options(&ctx, query.as_str());
        });
    }
    {
        let ctx = ctx.clone_ctx();
        metronome_window.on_pick_output(move |key| {
            dispatch(
                &ctx,
                Command::Metronome(MetronomeCommand::SetMetronomeOutput {
                    device_id: Some(key.to_string()),
                }),
            );
        });
    }
}

/// Refresh the cached device list from the host and return the ids. On an
/// enumeration failure the previous snapshot is kept — a transient host error
/// must not blank a select the user is looking at.
pub(crate) fn output_device_ids(cache: &Rc<RefCell<Vec<AudioDeviceDescriptor>>>) -> Vec<String> {
    match infra_cpal::list_output_device_descriptors() {
        Ok(devices) => {
            let ids = devices.iter().map(|d| d.id.clone()).collect();
            *cache.borrow_mut() = devices;
            ids
        }
        Err(e) => {
            log::warn!("[metronome] output device enumeration failed: {e}");
            cache.borrow().iter().map(|d| d.id.clone()).collect()
        }
    }
}

/// Publish the (filtered) device rows onto the select. Filtering lives here
/// because Slint has no string `contains`.
fn publish_output_options(ctx: &MetronomeCtx, query: &str) {
    let Some(mw) = ctx.window.upgrade() else {
        return;
    };
    let devices = ctx.devices.borrow();
    let options: Vec<SelectOption> = filter_output_devices(&devices, query)
        .into_iter()
        .map(|d| SelectOption {
            key: SharedString::from(d.id.as_str()),
            label: SharedString::from(d.name.as_str()),
        })
        .collect();
    mw.set_output_options(ModelRc::new(VecModel::from(options)));
}

/// Case-insensitive substring match on the device name, original order kept.
/// An empty (trimmed) query returns every device.
pub fn filter_output_devices<'a>(
    devices: &'a [AudioDeviceDescriptor],
    query: &str,
) -> Vec<&'a AudioDeviceDescriptor> {
    let needle = query.trim().to_lowercase();
    devices
        .iter()
        .filter(|d| needle.is_empty() || d.name.to_lowercase().contains(&needle))
        .collect()
}

#[cfg(test)]
#[path = "metronome_controls_wiring_tests.rs"]
mod tests;
