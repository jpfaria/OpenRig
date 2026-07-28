//! Issue #323 — turn looper events into controller-store mutations.
//!
//! A dispatch alone is dead (#614): the dispatcher records intent and emits an
//! event; THIS is where the controller's looper store learns about it. State
//! lives in the store (off the volatile chain runtime), so every op below is a
//! deterministic, immediate mutation — no queue to an audio callback that may
//! not be running, no suppression race.

use std::cell::RefCell;

use application::command::{LooperAction, LooperParam};
use application::event::Event;
use infra_cpal::ProjectRuntimeController;
use project::chain::Chain;
use project::rig::RigProject;

/// Apply one looper event to the controller's looper store. Unknown events are
/// ignored, so callers can hand it the whole event stream.
pub fn apply_looper_event(controller: &ProjectRuntimeController, event: &Event) {
    match event {
        Event::ChainLooperAdded { chain, looper } => {
            controller.looper_create(chain, *looper);
        }

        Event::ChainLooperRemoved { chain, looper } => {
            controller.looper_remove(chain, *looper);
        }

        Event::ChainLooperTransportChanged {
            chain,
            looper,
            action,
        } => {
            let uid = *looper;
            match action {
                LooperAction::Record => controller.looper_tap_record(chain, uid),
                LooperAction::Stop => controller.looper_stop(chain, uid),
                LooperAction::Play => controller.looper_play(chain, uid),
                LooperAction::PlayStop => {
                    // One button, both actions — the store's current state decides.
                    if controller.looper_is_playing(chain, uid) {
                        controller.looper_stop(chain, uid);
                    } else {
                        controller.looper_play(chain, uid);
                    }
                }
                LooperAction::Undo => controller.looper_undo(chain, uid),
                LooperAction::Redo => controller.looper_redo(chain, uid),
                LooperAction::Clear => controller.looper_clear(chain, uid),
            }
        }

        Event::ChainLooperParamChanged {
            chain,
            looper,
            param,
        } => {
            let uid = *looper;
            match *param {
                LooperParam::Mix(v) => controller.looper_set_mix(chain, uid, v),
                LooperParam::Decay(v) => controller.looper_set_decay(chain, uid, v),
                LooperParam::Speed(s) => controller.looper_set_speed(chain, uid, s),
                LooperParam::Reverse(v) => controller.looper_set_reverse(chain, uid, v),
            }
        }

        _ => {}
    }
}

/// #323 phase 2: resolve each looper's LINKED preset into the effect blocks it
/// plays through and push them into the controller store, so a Playing loop
/// renders through its fixed tone rather than the chain's current preset. A
/// looper with no link (or a non-rig session) clears its override, falling back
/// to the chain's blocks. Called on the meter tick, just before the isolated
/// playback streams are reconciled; the store bumps its re-arm generation only
/// on a real change, so a steady loop never respawns.
pub fn sync_looper_playback_presets(
    controller: &ProjectRuntimeController,
    chain: &Chain,
    rig: Option<&RefCell<RigProject>>,
) {
    // The rig input name is the chain id minus the `rig:` projection prefix; a
    // non-rig chain has neither a rig nor a linked preset.
    let input_name = chain.id.0.strip_prefix("rig:");
    for cfg in &chain.loopers {
        let blocks = match (cfg.preset.as_deref(), input_name, rig) {
            (Some(preset_id), Some(input), Some(rig)) => {
                engine::rig_runtime::looper_playback_blocks(&rig.borrow(), input, preset_id)
            }
            _ => None,
        };
        controller.looper_set_playback_blocks(&chain.id, cfg.uid, blocks);
    }
}
