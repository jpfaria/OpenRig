//! Responsibility: maps every drum machine control to its command.
//!
//! The panel is a pure dispatcher: each callback becomes one `DrumsCommand`
//! and the dispatcher validates, applies and persists. The same mapping serves
//! the drums window, the inline overlay and the compact view's section.

use std::rc::Rc;

use application::command::DrumsCommand;

use crate::DrumsBridge;

/// Where a drum machine control sends its command.
pub(crate) type DrumsDispatch = Rc<dyn Fn(DrumsCommand)>;

/// Route every control on `bridge` through `dispatch`.
pub(crate) fn wire_drums_intents(bridge: &DrumsBridge, dispatch: DrumsDispatch) {
    let d = dispatch.clone();
    // POWER is the one transport switch: on plays, off stops and closes.
    bridge.on_toggle_enabled(move |on| {
        d(if on {
            DrumsCommand::PlayDrums
        } else {
            DrumsCommand::SetDrumsEnabled { enabled: false }
        })
    });
    let d = dispatch.clone();
    bridge.on_fill(move || d(DrumsCommand::TriggerDrumFill));
    let d = dispatch.clone();
    bridge.on_set_bpm(move |bpm| d(DrumsCommand::SetDrumsBpm { bpm }));
    let d = dispatch.clone();
    bridge.on_set_volume(move |volume| d(DrumsCommand::SetDrumsVolume { volume }));
    let d = dispatch.clone();
    bridge.on_pick_kit(move |kit| {
        d(DrumsCommand::SelectDrumKit {
            kit: kit.to_string(),
        })
    });
    let d = dispatch.clone();
    bridge.on_pick_groove(move |groove| {
        d(DrumsCommand::SelectDrumGroove {
            groove: groove.to_string(),
        })
    });
    bridge.on_pick_output(move |key| {
        dispatch(DrumsCommand::SetDrumsOutput {
            output_key: Some(key.to_string()),
        })
    });
}

#[cfg(test)]
#[path = "drums_intents_tests.rs"]
mod tests;
