//! Issue #323 — turn looper events into controller-store mutations.
//!
//! A dispatch alone is dead (#614): the dispatcher records intent and emits an
//! event; THIS is where the controller's looper store learns about it. State
//! lives in the store (off the volatile chain runtime), so every op below is a
//! deterministic, immediate mutation — no queue to an audio callback that may
//! not be running, no suppression race.

use application::command::{LooperAction, LooperParam};
use application::event::Event;
use infra_cpal::ProjectRuntimeController;

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
