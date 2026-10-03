//! Responsibility: routes the player panel's controls to the dispatcher.
//!
//! A control dispatches its `Command` and redraws what the dispatcher now
//! holds; it never touches the session, the config or the audio runtime.

use std::path::PathBuf;

use application::command::{Command, PlayerCommand};
use slint::{ModelRc, SharedString, VecModel};

use crate::metronome_controls_wiring::refresh_metronome_outputs;
use crate::player_file_chooser::choose_backing_track;
use crate::player_render::{render_library, render_reading, render_snapshot};
use crate::player_view::{loop_press, LoopPress};
use crate::player_wiring::PlayerCtx;
use crate::{PlayerBridge, SelectOption};

/// Dispatch a player command and redraw from the state it produced.
pub(crate) fn dispatch(ctx: &PlayerCtx, cmd: Command) {
    let result = {
        let borrowed = ctx.project_session.borrow();
        let Some(session) = borrowed.as_ref() else {
            return;
        };
        session.dispatcher.dispatch(cmd)
    };
    if let Err(e) = result {
        log::warn!("[player] dispatch failed: {e}");
    }
    render_snapshot(ctx);
    render_reading(ctx);
}

fn player(ctx: &PlayerCtx, cmd: PlayerCommand) {
    dispatch(ctx, Command::Player(cmd));
}

/// Connect every control on one surface's bridge.
pub(crate) fn wire_controls(bridge: &PlayerBridge, ctx: &PlayerCtx) {
    let c = ctx.clone_ctx();
    bridge.on_set_playing(move |playing| player(&c, PlayerCommand::SetPlayerPlaying { playing }));
    let c = ctx.clone_ctx();
    bridge.on_stop(move || player(&c, PlayerCommand::StopPlayer));
    let c = ctx.clone_ctx();
    bridge.on_seek(move |seconds| {
        player(
            &c,
            PlayerCommand::SeekPlayer {
                seconds: f64::from(seconds),
            },
        )
    });
    let c = ctx.clone_ctx();
    bridge.on_set_volume(move |volume| player(&c, PlayerCommand::SetPlayerVolume { volume }));
    let c = ctx.clone_ctx();
    bridge.on_set_speed(move |speed| player(&c, PlayerCommand::SetPlayerSpeed { speed }));
    let c = ctx.clone_ctx();
    bridge.on_set_semitones(move |semitones| {
        player(&c, PlayerCommand::SetPlayerSemitones { semitones })
    });
    let c = ctx.clone_ctx();
    bridge.on_mark_loop(move || mark_loop(&c));
    let c = ctx.clone_ctx();
    bridge.on_pick_track(move |path| load(&c, PathBuf::from(path.as_str())));
    let c = ctx.clone_ctx();
    bridge.on_pick_category(move |key| {
        c.for_each_bridge(|bridge| bridge.set_category(key.clone()));
        render_library(&c);
    });
    let c = ctx.clone_ctx();
    bridge.on_play_track(move |path| {
        load(&c, PathBuf::from(path.as_str()));
        player(&c, PlayerCommand::SetPlayerPlaying { playing: true });
    });
    let c = ctx.clone_ctx();
    bridge.on_delete_track(move |path| {
        player(
            &c,
            PlayerCommand::DeletePlayerTrack {
                path: PathBuf::from(path.as_str()),
            },
        );
        render_library(&c);
    });
    let c = ctx.clone_ctx();
    bridge.on_choose_file(move || {
        if let Some(path) = choose_backing_track() {
            load(&c, path);
        }
    });
    let c = ctx.clone_ctx();
    bridge.on_pick_output(move |key| {
        player(
            &c,
            PlayerCommand::SetPlayerOutput {
                device_id: Some(key.to_string()),
            },
        )
    });
    let c = ctx.clone_ctx();
    bridge.on_output_opened(move || publish_output_options(&c));
}

/// A new track forgets a loop start marked on the old one.
fn load(ctx: &PlayerCtx, path: PathBuf) {
    ctx.loop_mark.set(None);
    player(ctx, PlayerCommand::LoadPlayerTrack { path });
}

fn mark_loop(ctx: &PlayerCtx) {
    let now = ctx.live.player().map_or(0.0, |r| r.position_seconds);
    let loop_set = ctx
        .snapshot()
        .is_some_and(|s| s.settings.loop_range.is_some());
    match loop_press(ctx.loop_mark.get(), now, loop_set) {
        LoopPress::Mark(at) => {
            ctx.loop_mark.set(Some(at));
            render_snapshot(ctx);
        }
        LoopPress::Set { start, end } => {
            ctx.loop_mark.set(None);
            player(
                ctx,
                PlayerCommand::SetPlayerLoop {
                    start_seconds: start,
                    end_seconds: end,
                },
            );
        }
        LoopPress::Clear => player(ctx, PlayerCommand::ClearPlayerLoop),
    }
}

/// Re-read the project's endpoints so one added since the last look shows.
fn publish_output_options(ctx: &PlayerCtx) {
    let options: Vec<SelectOption> = refresh_metronome_outputs(&ctx.project_session, &ctx.outputs)
        .into_iter()
        .map(|o| SelectOption {
            key: SharedString::from(o.key.as_str()),
            label: SharedString::from(o.label.as_str()),
        })
        .collect();
    ctx.for_each_bridge(|bridge| {
        bridge.set_output_options(ModelRc::new(VecModel::from(options.clone())));
    });
}
