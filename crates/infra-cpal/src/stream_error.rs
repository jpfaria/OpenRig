//! Responsibility: decides what an error reported by a cpal stream does.

/// What a cpal stream error leads to.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum StreamErrorAction {
    /// Nothing: not an error OpenRig acts on, and reported where acting would
    /// break the audio thread's rules.
    Ignore,
    /// Logged, as every stream error was before cpal 0.18.
    Log,
}

/// The action for an error of `kind`.
///
/// cpal 0.18 reports device overloads (`Xrun`) through the error callback, on
/// CoreAudio from the real-time I/O thread. cpal 0.17 never reported them,
/// and logging there would allocate and lock on the audio thread (invariant
/// #8), so they are dropped (#978). cpal delivers the other kinds from its
/// own non-real-time threads, and they are logged as before.
pub(crate) fn action_for(kind: cpal::ErrorKind) -> StreamErrorAction {
    match kind {
        cpal::ErrorKind::Xrun => StreamErrorAction::Ignore,
        _ => StreamErrorAction::Log,
    }
}

/// The error callback of one stream. `context` names the stream in the log
/// line; it is built here, at stream build time, never on the audio thread.
pub(crate) fn stream_error_handler(context: String) -> impl FnMut(cpal::Error) + Send + 'static {
    move |err| match action_for(err.kind()) {
        StreamErrorAction::Ignore => {}
        StreamErrorAction::Log => log::error!("{context}: {err}"),
    }
}

#[cfg(test)]
#[path = "stream_error_tests.rs"]
mod tests;
