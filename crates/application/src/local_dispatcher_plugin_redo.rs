//! Responsibility: rebuilds an owned plugin's parameters from its capture names.
//!
//! A TONE3000 install with a key set asks TONE3000 first, on a worker
//! thread: a newer tone is downloaded again, a current one gives its names.
//! Every other plugin uses the names it recorded, or its file names.

use std::path::Path;

use anyhow::{anyhow, Result};
use plugin_loader::manifest::PluginManifest;
use project::block::grid_version_follow::capture_grid;

use crate::event::{Event, PluginLibraryEvent};
use crate::local_dispatcher::{AsyncDone, LocalDispatcher};
use crate::plugin_library::capture_names::capture_names;
use crate::plugin_library::disk_manifest::read_disk_manifest;
use crate::plugin_library::manifest_save::save_manifest_version;
use crate::plugin_library::redo_worker::{redo_tone, PluginLibraryDone, RedoTone};
use crate::plugin_library::reinfer::reinfer;
use crate::plugin_library::PluginOrigin;
use crate::tone3000::catalog_tones::manifest_tone_ids;

impl LocalDispatcher {
    pub(crate) fn redo_plugin_parameters(&self, plugin_id: String) -> Result<Vec<Event>> {
        let (package, origin) = self.plugin_library_roots().owned(&plugin_id)?;
        let root = package.root.clone();
        let manifest = read_disk_manifest(&root)?;
        let tone_id = manifest_tone_ids(&manifest).first().copied();
        let (api, tone3000_root) = {
            let state = self.tone3000.borrow().clone();
            let state = state.borrow();
            (state.api(), state.root())
        };
        if let (PluginOrigin::Tone3000, Some(api), Some(tone3000_root), Some(tone_id)) =
            (origin, api, tone3000_root, tone_id)
        {
            let job = RedoTone {
                plugin_id,
                tone_id,
                tone3000_root,
                package_root: root,
                manifest,
            };
            let tx = self.async_done_tx.clone();
            std::thread::Builder::new()
                .name("plugin-redo".into())
                .spawn(move || {
                    let _ = tx.send(AsyncDone::PluginLibrary(redo_tone(api.as_ref(), job)));
                })
                .map_err(|e| anyhow!("failed to spawn plugin-redo task: {e}"))?;
            return Ok(vec![]);
        }
        let captures = capture_grid(&manifest)
            .map(|(_, captures)| captures)
            .ok_or_else(|| anyhow!("`{plugin_id}` has no capture parameters to rebuild"))?;
        let names = capture_names(&root, captures);
        self.redo_with_names(plugin_id, &root, &manifest, &names)
    }

    /// Saves the parameters `names` infer as the next version.
    fn redo_with_names(
        &self,
        plugin_id: String,
        root: &Path,
        current: &PluginManifest,
        names: &[String],
    ) -> Result<Vec<Event>> {
        let next = reinfer(current, names).ok_or_else(|| {
            anyhow!("the capture names of `{plugin_id}` do not match its captures")
        })?;
        let version = save_manifest_version(root, current, &next)?;
        let mut events = self.follow_plugin_blocks(current, &next);
        events.push(Event::PluginLibrary(PluginLibraryEvent::Redone {
            plugin_id,
            version,
        }));
        Ok(events)
    }

    /// Applies one redo worker report and names what changed.
    pub(crate) fn finish_plugin_library(&self, done: PluginLibraryDone) -> Vec<Event> {
        let result = match done {
            PluginLibraryDone::Names { plugin_id, names } => {
                let redone: Result<Vec<Event>> = (|| {
                    let (package, _) = self.plugin_library_roots().owned(&plugin_id)?;
                    let root = package.root.clone();
                    let current = read_disk_manifest(&root)?;
                    self.redo_with_names(plugin_id.clone(), &root, &current, &names)
                })();
                redone.map_err(|e| (plugin_id, format!("{e:#}")))
            }
            PluginLibraryDone::Updated {
                plugin_id,
                from,
                to,
                version,
            } => {
                self.tone3000.borrow().borrow_mut().refresh_installed();
                let mut events = self.follow_plugin_blocks(&from, &to);
                events.push(Event::PluginLibrary(PluginLibraryEvent::Redone {
                    plugin_id,
                    version,
                }));
                Ok(events)
            }
            PluginLibraryDone::Failed { plugin_id, message } => Err((plugin_id, message)),
        };
        result.unwrap_or_else(|(plugin_id, message)| {
            vec![Event::PluginLibrary(PluginLibraryEvent::RedoFailed {
                plugin_id,
                message,
            })]
        })
    }
}
