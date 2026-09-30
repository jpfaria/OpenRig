//! Responsibility: mirrors the global mixer onto the MIDI controller that drives it.
//! #1007 — with a `--midi=PATH` map that binds mixer strips, a fader or mute
//! changed anywhere (GUI, MCP, the surface itself) is sent back to the
//! controller so motor faders and MUTE LEDs follow. Polls on the GUI thread;
//! the audio thread is never involved.

use std::cell::RefCell;
use std::path::Path;
use std::rc::Rc;
use std::time::Duration;

use adapter_midi::feedback_output::FeedbackOutput;
use adapter_midi::mixer_feedback::strip_feedback;
use adapter_midi::mixer_feedback_tracker::MixerFeedbackTracker;
use adapter_midi::{Binding, MidiMap};
use slint::{Timer, TimerMode};

use crate::state::ProjectSession;

const POLL: Duration = Duration::from_millis(50);

/// Start the feedback poll, or `None` when the map binds no mixer strip or
/// no output of the named controller can be opened.
pub(crate) fn start(
    map_path: &Path,
    project_session: Rc<RefCell<Option<ProjectSession>>>,
) -> Option<Timer> {
    let map = MidiMap::load(map_path)
        .map_err(|e| log::warn!("mixer feedback: {e:#}"))
        .ok()?;
    let bindings: Vec<Binding> = map
        .bindings
        .into_iter()
        .filter(|b| b.command.contains("Mixer"))
        .collect();
    if bindings.is_empty() {
        return None;
    }
    let mut output = FeedbackOutput::open(map.input.as_deref())
        .map_err(|e| log::warn!("mixer feedback disabled: {e:#}"))
        .ok()?;
    let mut tracker = MixerFeedbackTracker::default();
    let timer = Timer::default();
    timer.start(TimerMode::Repeated, POLL, move || {
        let strips = match project_session.borrow().as_ref() {
            Some(session) => session.dispatcher.mixer_strips(),
            None => return,
        };
        for strip in strips {
            if tracker.changed(&strip.id, strip.gain_db, strip.muted, strip.soloed) {
                for message in strip_feedback(
                    &bindings,
                    &strip.id,
                    strip.gain_db,
                    strip.muted,
                    strip.soloed,
                ) {
                    output.send(&message);
                }
            }
        }
    });
    Some(timer)
}
