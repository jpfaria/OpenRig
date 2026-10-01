//! Responsibility: decides whether a drained event batch needs the chain views re-projected.
//!
//! #1007 — a fader moved from a control surface (SMC-Mixer through the Mackie
//! bridge over MCP, or a MIDI CC) arrives as one command per step of its
//! travel. Re-projecting the chain list (a model reset) and the open compact
//! view (a new block model) on every step tears down and rebuilds every card,
//! so the screen jumps while the fader moves. A fader step changes nothing
//! those views draw: the mixer surfaces follow it in place through their own
//! poll, and a chain volume is patched onto its card in place.

use application::event::Event;

/// True when at least one event in the batch changes what the chain list or
/// the compact view draws. A `ProjectMutated` right after a fader event is
/// that fader command's own echo; anywhere else it counts as a real edit.
pub(crate) fn batch_requires_view_rebuild(events: &[Event]) -> bool {
    let mut after_fader = false;
    for event in events {
        let fader = is_fader_event(event);
        let fader_echo = after_fader && matches!(event, Event::ProjectMutated);
        if !fader && !fader_echo {
            return true;
        }
        after_fader = fader;
    }
    false
}

fn is_fader_event(event: &Event) -> bool {
    matches!(
        event,
        Event::MixerStripChanged { .. }
            | Event::ChainMixerStripChanged { .. }
            | Event::ChainDiFaderChanged { .. }
            | Event::ChainVolumeChanged { .. }
    )
}
