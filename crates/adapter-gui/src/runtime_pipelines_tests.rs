//! #127: the click is "playing" only once its stream is proven open.
//!
//! `enabled` is not a request. Every reader treats it as the truth about
//! whether anything is audible: the beat lamps' 33 ms timer
//! (`metronome_wiring::start_lamp_timer` → `LiveSource::metronome`), the read
//! resolver behind `openrig://metronome` (`application::read::metronome_state`,
//! where the live flag WINS over the dispatcher's snapshot) and
//! `gui_live_source::metronome_reading` itself. Setting it before the open
//! means a device that is gone or busy leaves POWER lit over silence: the
//! dispatcher records `running=false` on the `Err`, `wire_power` draws the
//! switch off — and 33 ms later the lamp timer lights it again from the
//! generator's own flag, beat frozen, no sound, forever.
//!
//! These run in the normal suite: they exercise the ORDER around the open, not
//! the open, so they touch no audio host and enumerate no device. The full
//! path through a real `ProjectRuntimeController` is in
//! `metronome_runtime_tests.rs`, behind `OPENRIG_HW_TESTS=1` (#670).

use anyhow::bail;
use engine::metronome_state::{MetronomeSettings, MetronomeShared};

use super::mark_click_playing;

#[test]
fn a_click_whose_stream_never_opened_is_not_marked_playing() {
    let shared = MetronomeShared::new(MetronomeSettings::default());

    let result = mark_click_playing(&shared, || {
        bail!("metronome output device 'openrig-test-no-such-device' not found")
    });

    assert!(
        result.is_err(),
        "the caller must get the reason the click cannot sound: {result:?}"
    );
    assert!(
        !shared.enabled(),
        "#127: a click whose stream never opened must not read as playing — the lamp \
         timer and `openrig://metronome` resolve `running` from this flag, so a switch \
         that looks on over silence is exactly what it would show"
    );
}

#[test]
fn a_click_whose_stream_opened_plays_from_beat_one() {
    let shared = MetronomeShared::new(MetronomeSettings::default());

    mark_click_playing(&shared, || Ok(())).expect("the stream opened");

    assert!(shared.enabled(), "an open stream is a playing click");
    assert!(
        shared.take_restart(),
        "the bar starts at beat one, not wherever the previous run left the phase"
    );
}
