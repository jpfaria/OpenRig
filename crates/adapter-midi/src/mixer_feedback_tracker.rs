//! Responsibility: remembers the last mixer state sent to a surface.
//! #1007 — the feedback poll asks "did this strip change since I last told
//! the surface?" so an idle mixer sends nothing and a moved fader sends once.

use std::collections::HashMap;

#[derive(Debug, Default)]
pub struct MixerFeedbackTracker {
    last: HashMap<String, (f32, bool)>,
}

impl MixerFeedbackTracker {
    /// `true` (and remembers the new state) when `strip` is new or its gain
    /// or mute differs from what was last reported.
    pub fn changed(&mut self, strip: &str, gain_db: f32, muted: bool) -> bool {
        let state = (gain_db, muted);
        if self.last.get(strip) == Some(&state) {
            return false;
        }
        self.last.insert(strip.to_string(), state);
        true
    }
}

#[cfg(test)]
#[path = "mixer_feedback_tracker_tests.rs"]
mod tests;
