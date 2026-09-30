//! Responsibility: restores the global mixer into a session's dispatcher.
//! #1007: the strips' faders and mutes live in the per-machine `config.yaml`
//! (ADR 0003). They are restored AFTER the session's I/O bindings are
//! installed, so a mono input strip fans out to every split pipeline from the
//! first callback.

use std::cell::RefCell;
use std::path::PathBuf;
use std::rc::Rc;

use application::dispatcher::CommandDispatcher;
use application::mixer_state::MixerControlState;
use infra_filesystem::{FilesystemStorage, MixerStripConfig};

/// Restore the machine's mixer settings into `dispatcher`. Call once the
/// session's bindings are in place.
pub(crate) fn restore_mixer_state(dispatcher: &dyn CommandDispatcher) {
    let config = FilesystemStorage::load_app_config().unwrap_or_default();
    attach_restored(dispatcher, &config.mixer, mixer_config_path());
}

/// Hand `dispatcher` the given settings, persisting back to `path`.
pub(crate) fn attach_restored(
    dispatcher: &dyn CommandDispatcher,
    strips: &[MixerStripConfig],
    path: Option<PathBuf>,
) {
    dispatcher.attach_mixer_state(Rc::new(RefCell::new(MixerControlState::restored(
        strips, path,
    ))));
}

/// Where a fader move persists: the machine's `config.yaml`. `None` in a test
/// build, structurally (#701) — no test can rewrite the user's mixer.
#[cfg(not(test))]
fn mixer_config_path() -> Option<PathBuf> {
    FilesystemStorage::app_config_path().ok()
}

#[cfg(test)]
fn mixer_config_path() -> Option<PathBuf> {
    None
}
