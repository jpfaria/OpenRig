//! Responsibility: mirrors the player's state onto the panel's bridge.

use application::player_library::{list_backing_tracks, BackingTrack};
use slint::{ModelRc, SharedString, VecModel};

use crate::metronome_controls_wiring::refresh_metronome_outputs;
use crate::metronome_view::resolve_output_endpoint;
use crate::player_category_view::{category_tabs, selected_category, tracks_in};
use crate::player_view::{format_clock, loop_label, semitones_label, speed_label, track_name};
use crate::player_wiring::PlayerCtx;
use crate::PlayerTrackRow;

/// Draw what the dispatcher holds: the track, the knobs, the loop and the
/// output. With no project open there is nothing to draw.
pub(crate) fn render_snapshot(ctx: &PlayerCtx) {
    let Some(state) = ctx.snapshot() else {
        return;
    };
    let endpoints = refresh_metronome_outputs(&ctx.project_session, &ctx.outputs);
    let resolved = resolve_output_endpoint(state.output_key.as_deref(), &endpoints);
    let output_key = resolved.as_ref().map(|o| o.key.clone()).unwrap_or_default();
    let output_label = resolved.map(|o| o.label).unwrap_or_default();
    let name = state.track.as_deref().map(track_name).unwrap_or_default();
    let path = state
        .track
        .as_deref()
        .map(|p| p.to_string_lossy().into_owned())
        .unwrap_or_default();
    let settings = state.settings;
    let mark = ctx.loop_mark.get();
    let (loop_start, loop_end) = settings.loop_range.unwrap_or((0.0, 0.0));
    let loop_text = loop_label(settings.loop_range, mark);
    ctx.for_each_bridge(|bridge| {
        bridge.set_track_name(SharedString::from(name.as_str()));
        bridge.set_track_path(SharedString::from(path.as_str()));
        bridge.set_volume(settings.volume);
        bridge.set_speed(settings.speed);
        bridge.set_semitones(settings.semitones);
        bridge.set_speed_label(SharedString::from(speed_label(settings.speed)));
        bridge.set_semitones_label(SharedString::from(semitones_label(settings.semitones)));
        bridge.set_loop_active(settings.loop_range.is_some());
        bridge.set_loop_start(loop_start as f32);
        bridge.set_loop_end(loop_end as f32);
        bridge.set_loop_mark(mark.map_or(-1.0, |m| m as f32));
        bridge.set_loop_label(SharedString::from(loop_text.as_str()));
        bridge.set_output_key(SharedString::from(output_key.as_str()));
        bridge.set_output_label(SharedString::from(output_label.as_str()));
    });
    *ctx.rendered.borrow_mut() = Some(state);
}

/// Draw where the track is, as its own stream reports it. No runtime hosted
/// means nothing is playing.
pub(crate) fn render_reading(ctx: &PlayerCtx) {
    let reading = ctx.live.player().unwrap_or_default();
    let position = format_clock(reading.position_seconds);
    let duration = format_clock(reading.duration_seconds);
    ctx.for_each_bridge(|bridge| {
        bridge.set_playing(reading.playing);
        bridge.set_loading(reading.loading);
        bridge.set_failed(reading.failed);
        bridge.set_position(reading.position_seconds as f32);
        bridge.set_duration(reading.duration_seconds as f32);
        bridge.set_position_label(SharedString::from(position.as_str()));
        bridge.set_duration_label(SharedString::from(duration.as_str()));
    });
}

/// List the selected category's tracks, bundled and the user's.
pub(crate) fn render_library(ctx: &PlayerCtx) {
    let Some(dirs) = ctx
        .project_session
        .borrow()
        .as_ref()
        .map(|session| session.dispatcher.player_library())
    else {
        return;
    };
    let tracks = list_backing_tracks(&dirs);
    let tabs: Vec<SharedString> = category_tabs(&tracks)
        .into_iter()
        .map(|category| SharedString::from(category.key()))
        .collect();
    let folder = dirs
        .user
        .as_deref()
        .map(|p| p.to_string_lossy().into_owned())
        .unwrap_or_default();
    ctx.for_each_bridge(|bridge| {
        let category = selected_category(bridge.get_category().as_str());
        let rows: Vec<PlayerTrackRow> = tracks_in(&tracks, category).map(track_row).collect();
        bridge.set_category(SharedString::from(category.key()));
        bridge.set_categories(ModelRc::new(VecModel::from(tabs.clone())));
        bridge.set_library(ModelRc::new(VecModel::from(rows)));
        bridge.set_user_folder(SharedString::from(folder.as_str()));
    });
}

fn track_row(track: &BackingTrack) -> PlayerTrackRow {
    PlayerTrackRow {
        name: SharedString::from(track.name.as_str()),
        path: SharedString::from(track.path.to_string_lossy().as_ref()),
        bundled: track.bundled,
        category: SharedString::from(track.category.key()),
    }
}
