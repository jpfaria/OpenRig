use super::{action_for, StreamErrorAction};

#[test]
fn an_xrun_is_not_logged() {
    // cpal 0.18 reports device overloads through the error callback, on
    // CoreAudio from the real-time I/O thread; logging there allocates and
    // locks on the audio thread (invariant #8). cpal 0.17 never reported them.
    assert_eq!(action_for(cpal::ErrorKind::Xrun), StreamErrorAction::Ignore);
}

#[test]
fn a_lost_or_invalidated_stream_is_still_logged() {
    for kind in [
        cpal::ErrorKind::StreamInvalidated,
        cpal::ErrorKind::DeviceNotAvailable,
        cpal::ErrorKind::BackendError,
        cpal::ErrorKind::RealtimeDenied,
    ] {
        assert_eq!(action_for(kind), StreamErrorAction::Log, "{kind:?}");
    }
}
