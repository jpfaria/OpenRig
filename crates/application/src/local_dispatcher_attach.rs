//! `LocalDispatcher` dependency-attachment setters (issue #792 split).
//!
//! Single responsibility: wiring. The session bootstrap hands the dispatcher
//! its shared rig handle, the resolved library/project/config paths, and the
//! live engine sample rate. No command handling, no reads.

use std::cell::RefCell;
use std::path::PathBuf;
use std::rc::Rc;

use domain::ids::ChainId;
use project::rig::RigProject;

use crate::local_dispatcher::LocalDispatcher;

impl LocalDispatcher {
    /// Share the session's `RigProject` handle so rig-nav commands can
    /// mutate the same allocation the GUI renders from. Idempotent.
    ///
    /// #127: `pub(crate)` — the crate's public surface is
    /// `CommandDispatcher::attach_rig` (see `local_dispatcher_trait.rs`),
    /// which delegates here.
    pub(crate) fn attach_rig(&self, rig: Rc<RefCell<RigProject>>) {
        *self.rig.borrow_mut() = Some(rig);
    }

    /// #555: configure the preset library directory. Called by the
    /// session bootstrap once the resolved `presets_path` is known.
    /// Idempotent — calling this again replaces the path.
    pub(crate) fn attach_presets_path(&self, path: PathBuf) {
        *self.presets_path.borrow_mut() = Some(path);
    }

    /// #555: configure where `Command::SaveProject` writes the project
    /// file. Called by the session bootstrap and again on every "Save
    /// As" so the dispatcher and the GUI agree on the current target.
    pub(crate) fn attach_project_path(&self, path: PathBuf) {
        *self.project_path.borrow_mut() = Some(path);
    }

    /// #555: optional override for the sidecar `config.yaml` path.
    /// `None` ⇒ the dispatcher derives it from `project_path.parent()
    /// .join("config.yaml")` at save time (matches the pre-#555
    /// behaviour). Idempotent.
    pub(crate) fn attach_config_path(&self, path: Option<PathBuf>) {
        *self.config_path.borrow_mut() = path;
    }

    /// #792 / ADR-0003: override the per-machine SYSTEM config path (where the
    /// I/O binding registry persists). `None` ⇒ `FilesystemStorage::
    /// app_config_path()`. Production leaves this unset (the registry lands in
    /// the real OS config); tests attach a temp path so they never touch it.
    /// SEPARATE from `attach_config_path` — the project sidecar must never
    /// receive the per-machine registry.
    ///
    /// Not part of `CommandDispatcher` (#127 only moved the 11 GUI-facing
    /// methods listed in the task brief) — stays `pub` for direct callers.
    pub fn attach_io_config_path(&self, path: Option<PathBuf>) {
        *self.io_config_path.borrow_mut() = path;
    }

    /// #614/#669: inform the dispatcher of the engine sample rate so DI loop
    /// decoding resamples to the correct target. Call this once the audio
    /// stream is running and whenever the device rate changes. Defaults to
    /// 48 000 Hz.
    ///
    /// #749: DI sources are stored un-resampled (`DiPcm`) and resampled at ARM
    /// time, per output-stream rate — so the store needs no rebuild on a rate
    /// change. What DOES need attention is a loop that is PLAYING: its live
    /// runtime was rebuilt at the new rate, and a loop left from the old one
    /// drags in slow motion until it is re-armed.
    ///
    /// #127: that re-arm happens HERE, through
    /// [`crate::runtime_control::RuntimeControl::refresh_di_stream`] — one
    /// chain at a time, by identity, and only for a stream that is already
    /// sounding. The GUI used to run the loop itself after calling this, so a
    /// rate change that came from anywhere else left the loop dragging. The
    /// chains with a loaded source are still RETURNED, unchanged, for callers
    /// that track them. No-op when the rate is unchanged.
    pub(crate) fn attach_engine_sr(&self, sr: u32) -> Vec<ChainId> {
        if *self.engine_sr.borrow() == sr {
            return Vec::new();
        }
        *self.engine_sr.borrow_mut() = sr;
        let loaded: Vec<ChainId> = self.di_loop_state.borrow().keys().cloned().collect();
        if let Some(control) = self.runtime_control() {
            for chain in &loaded {
                let (Some(chain_def), Some(pcm)) = (self.chain_def(chain), self.di_pcm(chain))
                else {
                    continue;
                };
                if let Err(e) = control.refresh_di_stream(&chain_def, pcm) {
                    log::warn!("[di] re-arm of '{}' at {sr} Hz failed: {e}", chain.0);
                }
            }
        }
        loaded
    }
}
