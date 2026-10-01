//! Responsibility: turns mixer-strip gestures on a `MixerBridge` into mixer commands.
//! #1007 — the Mixer window and the compact chain view host the same strips;
//! both hand their bridge here, so a gesture means the same command anywhere.

use std::rc::Rc;

use application::command::MixerCommand;

use crate::mixer_fader_law::db_from_position;
use crate::MixerBridge;

/// Route the bridge's fader, reset, mute and solo gestures to `dispatch`.
pub(crate) fn wire_strip_intents(bridge: &MixerBridge, dispatch: Rc<dyn Fn(MixerCommand)>) {
    let d = dispatch.clone();
    bridge.on_fader_moved(move |id, position| {
        d(MixerCommand::SetMixerFader {
            strip: id.to_string(),
            gain_db: db_from_position(position),
        });
    });
    let d = dispatch.clone();
    bridge.on_fader_reset(move |id| {
        d(MixerCommand::SetMixerFader {
            strip: id.to_string(),
            gain_db: 0.0,
        });
    });
    let d = dispatch.clone();
    bridge.on_mute_toggled(move |id| {
        d(MixerCommand::ToggleMixerMute {
            strip: id.to_string(),
        });
    });
    bridge.on_solo_toggled(move |id| {
        dispatch(MixerCommand::ToggleMixerSolo {
            strip: id.to_string(),
        });
    });
}

#[cfg(test)]
#[path = "mixer_strip_intents_tests.rs"]
mod tests;
