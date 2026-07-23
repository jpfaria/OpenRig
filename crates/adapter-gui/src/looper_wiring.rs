//! Issue #323 — turn looper events into audio-thread ops.
//!
//! A dispatch alone is dead (#614): the dispatcher records intent and emits
//! an event; THIS is where the chain's runtimes learn about it. Layer buffers
//! are allocated here, on the GUI thread, and handed to the audio thread
//! inside the op — the audio thread never allocates (invariant #8).

use application::command::{LooperAction, LooperParam};
use application::event::Event;
use engine::runtime::ChainRuntimeState;
use engine::{LooperOp, LooperState};
use infra_cpal::ProjectRuntimeController;
use std::sync::Arc;

/// Whether a record tap in this state needs a fresh layer buffer: it does
/// when the tap STARTS something (a first recording or an overdub), and does
/// not when it closes what is already running.
fn tap_needs_layer(state: LooperState) -> bool {
    matches!(
        state,
        LooperState::Empty | LooperState::Playing | LooperState::Stopped
    )
}

fn fresh_layer(runtime: &Arc<ChainRuntimeState>) -> Box<[f32]> {
    vec![0.0f32; runtime.looper_max_frames() * 2].into_boxed_slice()
}

/// Apply one looper event to the chain's runtimes. Unknown events are ignored,
/// so callers can hand it the whole event stream.
pub fn apply_looper_event(controller: &ProjectRuntimeController, event: &Event) {
    match event {
        Event::ChainLooperAdded { chain, looper } => {
            let uid = *looper;
            controller.push_chain_looper_op(chain, |_| Some(LooperOp::Create { uid }));
        }

        Event::ChainLooperRemoved { chain, looper } => {
            let uid = *looper;
            controller.push_chain_looper_op(chain, |_| Some(LooperOp::Remove { uid }));
        }

        Event::ChainLooperTransportChanged {
            chain,
            looper,
            action,
        } => {
            let uid = *looper;
            controller.push_chain_looper_op(chain, |runtime| {
                Some(match action {
                    LooperAction::Record => {
                        // Each runtime decides for ITSELF whether this tap
                        // starts or closes a recording — they are independent
                        // pipelines and may not be in the same state.
                        let state = runtime
                            .looper_status(uid)
                            .map_or(LooperState::Empty, |s| s.state);
                        LooperOp::TapRecord {
                            uid,
                            buffer: tap_needs_layer(state).then(|| fresh_layer(runtime)),
                        }
                    }
                    LooperAction::Play => LooperOp::Play { uid },
                    LooperAction::Stop => LooperOp::Stop { uid },
                    LooperAction::Undo => LooperOp::Undo { uid },
                    LooperAction::Redo => LooperOp::Redo { uid },
                    LooperAction::Clear => LooperOp::Clear { uid },
                })
            });
        }

        Event::ChainLooperParamChanged {
            chain,
            looper,
            param,
        } => {
            let uid = *looper;
            controller.push_chain_looper_op(chain, |_| {
                Some(match *param {
                    LooperParam::Mix(value) => LooperOp::SetMix { uid, value },
                    LooperParam::Decay(value) => LooperOp::SetDecay { uid, value },
                    LooperParam::Speed(speed) => LooperOp::SetSpeed { uid, speed },
                    LooperParam::Reverse(value) => LooperOp::SetReverse { uid, value },
                })
            });
        }

        _ => return,
    }

    // Free whatever the audio thread handed back (cleared layers, refused
    // buffers). Dropping here keeps `free` off the audio thread.
    if let Some(chain) = event.chain() {
        controller.drain_chain_looper_layers(chain);
    }
}
