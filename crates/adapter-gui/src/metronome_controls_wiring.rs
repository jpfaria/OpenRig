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
use slint::{ModelRc, SharedString, VecModel};

use crate::metronome_events::dispatch;
use crate::metronome_view::{
    output_endpoints, subdivision_key, timbre_key, time_signature_beats, MetronomeOutput,
};
use crate::metronome_wiring::MetronomeCtx;
use crate::state::ProjectSession;
use crate::{MetronomeBridge, SelectOption};

/// Connect the tempo row, the four knobs and the count-in pill on one surface's
/// bridge. Called once per surface (window + inline), so either fires the same
/// dispatch.
pub(crate) fn wire_controls(bridge: &MetronomeBridge, ctx: &MetronomeCtx) {
    {
        let ctx = ctx.clone_ctx();
        bridge.on_set_bpm(move |bpm| {
            dispatch(
                &ctx,
                Command::Metronome(MetronomeCommand::SetMetronomeBpm { bpm }),
            );
        });
    }
    {
        let ctx = ctx.clone_ctx();
        bridge.on_tap(move || {
            dispatch(&ctx, Command::Metronome(MetronomeCommand::MetronomeTap));
        });
    }
    {
        let ctx = ctx.clone_ctx();
        bridge.on_set_time_signature(move |index| {
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
        bridge.on_set_subdivision(move |index| {
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
        bridge.on_set_timbre(move |index| {
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
        bridge.on_set_volume(move |volume| {
            dispatch(
                &ctx,
                Command::Metronome(MetronomeCommand::SetMetronomeVolume { volume }),
            );
        });
    }
    {
        let ctx = ctx.clone_ctx();
        bridge.on_set_count_in(move |enabled| {
            dispatch(
                &ctx,
                Command::Metronome(MetronomeCommand::SetMetronomeCountIn { enabled }),
            );
        });
    }
}

// ── output device select ────────────────────────────────────────────────

pub(crate) fn wire_output_select(bridge: &MetronomeBridge, ctx: &MetronomeCtx) {
    {
        let ctx = ctx.clone_ctx();
        bridge.on_output_opened(move || {
            // Re-read the project's bindings on open so an endpoint added since
            // the last look shows up.
            refresh_metronome_outputs(&ctx.project_session, &ctx.outputs);
            publish_output_options(&ctx, "");
        });
    }
    {
        let ctx = ctx.clone_ctx();
        bridge.on_output_query(move |query| {
            publish_output_options(&ctx, query.as_str());
        });
    }
    {
        let ctx = ctx.clone_ctx();
        bridge.on_pick_output(move |key| {
            dispatch(
                &ctx,
                Command::Metronome(MetronomeCommand::SetMetronomeOutput {
                    device_id: Some(key.to_string()),
                }),
            );
        });
    }
}

/// Refresh the cached output-endpoint list from the project's I/O bindings and
/// return it. The metronome plays through the SAME outputs the project is
/// configured with (#14), not a raw device list.
pub(crate) fn refresh_metronome_outputs(
    project_session: &Rc<RefCell<Option<ProjectSession>>>,
    cache: &Rc<RefCell<Vec<MetronomeOutput>>>,
) -> Vec<MetronomeOutput> {
    let outputs = project_session
        .borrow()
        .as_ref()
        .map(|session| output_endpoints(&session.io_bindings.borrow()))
        .unwrap_or_default();
    *cache.borrow_mut() = outputs.clone();
    outputs
}

/// Publish the (filtered) endpoint rows onto every surface's select. Filtering
/// lives here because Slint has no string `contains`.
fn publish_output_options(ctx: &MetronomeCtx, query: &str) {
    let outputs = ctx.outputs.borrow();
    let options: Vec<SelectOption> = filter_outputs(&outputs, query)
        .into_iter()
        .map(|o| SelectOption {
            key: SharedString::from(o.key.as_str()),
            label: SharedString::from(o.label.as_str()),
        })
        .collect();
    // A fresh model per bridge — a ModelRc is single-owner, and both surfaces
    // read their own copy of the global.
    ctx.for_each_bridge(|bridge| {
        bridge.set_output_options(ModelRc::new(VecModel::from(options.clone())));
    });
}

/// Case-insensitive substring match on the endpoint label, original order kept.
/// An empty (trimmed) query returns every endpoint.
pub fn filter_outputs<'a>(outputs: &'a [MetronomeOutput], query: &str) -> Vec<&'a MetronomeOutput> {
    let needle = query.trim().to_lowercase();
    outputs
        .iter()
        .filter(|o| needle.is_empty() || o.label.to_lowercase().contains(&needle))
        .collect()
}

#[cfg(test)]
#[path = "metronome_controls_wiring_tests.rs"]
mod tests;
