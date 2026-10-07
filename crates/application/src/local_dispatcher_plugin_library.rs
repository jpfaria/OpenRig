//! Responsibility: handles the plugin library commands.
//!
//! A save, a restore and an uninstall run on the dispatching thread; they
//! touch one small manifest. A create copies and measures captures, and a
//! TONE3000 redo talks to the network, so those run on their own thread.

use std::path::PathBuf;

use anyhow::{anyhow, bail, Result};
use plugin_loader::manifest::PluginManifest;
use plugin_loader::version_store::read_version;
use project::block::capture_engine_params::capture_engine_parameter_names;
use project::block::grid_version_follow::capture_grid;

use crate::command::{Command, PluginLibraryCommand};
use crate::event::{Event, PluginLibraryEvent};
use crate::local_dispatcher::{AsyncDone, LocalDispatcher};
use crate::plugin_library::block_follow::follow_blocks;
use crate::plugin_library::create::{check_request, create_plugin, CreateRequest};
use crate::plugin_library::disk_manifest::read_disk_manifest;
use crate::plugin_library::grid_apply::apply_grid;
use crate::plugin_library::manifest_grid::replace_grid;
use crate::plugin_library::manifest_save::save_manifest_version;
use crate::plugin_library::{EditorGrid, PluginOrigin, PluginRoots};

impl LocalDispatcher {
    /// Adopt the plugins folder the frontend resolved for this session.
    pub fn attach_plugins_folder(&self, folder: Option<PathBuf>) {
        *self.plugins_folder.borrow_mut() = folder;
    }

    /// The folders whose plugins the user owns.
    pub fn plugin_library_roots(&self) -> PluginRoots {
        PluginRoots {
            plugins_folder: self.plugins_folder.borrow().clone(),
            tone3000: self.tone3000.borrow().borrow().root(),
        }
    }

    pub(crate) fn handle_plugin_library(&self, cmd: Command) -> Result<Vec<Event>> {
        let Command::PluginLibrary(cmd) = cmd else {
            bail!("handle_plugin_library called with a non-library command");
        };
        match cmd {
            PluginLibraryCommand::SavePluginParameters { plugin_id, grid } => {
                self.save_plugin_parameters(plugin_id, &grid)
            }
            PluginLibraryCommand::RestorePluginVersion { plugin_id, version } => {
                self.restore_plugin_version(plugin_id, version)
            }
            PluginLibraryCommand::RedoPluginParameters { plugin_id } => {
                self.redo_plugin_parameters(plugin_id)
            }
            PluginLibraryCommand::UninstallPlugin { plugin_id } => self.uninstall_plugin(plugin_id),
            PluginLibraryCommand::CreatePlugin {
                display_name,
                brand,
                block_type,
                backend,
                grid,
            } => self.create_plugin(CreateRequest {
                display_name,
                brand,
                block_type,
                backend,
                grid,
            }),
        }
    }

    fn save_plugin_parameters(&self, plugin_id: String, grid: &EditorGrid) -> Result<Vec<Event>> {
        let (package, _) = self.plugin_library_roots().owned(&plugin_id)?;
        let current = read_disk_manifest(&package.root)?;
        let (parameters, captures) = capture_grid(&current)
            .ok_or_else(|| anyhow!("`{plugin_id}` has no capture parameters to edit"))?;
        let (parameters, captures) = apply_grid(
            grid,
            parameters,
            captures,
            &capture_engine_parameter_names(),
        )?;
        let next = replace_grid(&current, parameters, captures)
            .ok_or_else(|| anyhow!("`{plugin_id}` has no capture parameters to edit"))?;
        let version = save_manifest_version(&package.root, &current, &next)?;
        let mut events = self.follow_plugin_blocks(&current, &next);
        events.push(Event::PluginLibrary(PluginLibraryEvent::Saved {
            plugin_id,
            version,
        }));
        Ok(events)
    }

    fn restore_plugin_version(&self, plugin_id: String, version: u32) -> Result<Vec<Event>> {
        let (package, _) = self.plugin_library_roots().owned(&plugin_id)?;
        let current = read_disk_manifest(&package.root)?;
        let Some(kept) = read_version(&package.root, version) else {
            bail!("`{plugin_id}` has no version {version}");
        };
        if kept.id != current.id {
            bail!(
                "version {version} of `{plugin_id}` belongs to `{}`",
                kept.id
            );
        }
        let saved = save_manifest_version(&package.root, &current, &kept)?;
        let mut events = self.follow_plugin_blocks(&current, &kept);
        events.push(Event::PluginLibrary(PluginLibraryEvent::Restored {
            plugin_id,
            version: saved,
        }));
        Ok(events)
    }

    fn uninstall_plugin(&self, plugin_id: String) -> Result<Vec<Event>> {
        let (package, origin) = self.plugin_library_roots().owned(&plugin_id)?;
        let root = package.root.clone();
        let _ = plugin_loader::registry::unload(&plugin_id);
        std::fs::remove_dir_all(&root)
            .map_err(|e| anyhow!("cannot remove {}: {e}", root.display()))?;
        if origin == PluginOrigin::Tone3000 {
            self.tone3000.borrow().borrow_mut().refresh_installed();
        }
        Ok(vec![Event::PluginLibrary(
            PluginLibraryEvent::Uninstalled { plugin_id },
        )])
    }

    fn create_plugin(&self, request: CreateRequest) -> Result<Vec<Event>> {
        check_request(&request)?;
        let Some(folder) = self.plugins_folder.borrow().clone() else {
            bail!("no plugins folder is attached");
        };
        let tx = self.async_done_tx.clone();
        std::thread::Builder::new()
            .name("plugin-create".into())
            .spawn(move || {
                let event = match create_plugin(&folder, &request) {
                    Ok(plugin_id) => PluginLibraryEvent::Created { plugin_id },
                    Err(e) => PluginLibraryEvent::CreateFailed {
                        message: format!("{e:#}"),
                    },
                };
                let _ = tx.send(AsyncDone::Events(vec![Event::PluginLibrary(event)]));
            })
            .map_err(|e| anyhow!("failed to spawn plugin-create task: {e}"))?;
        Ok(vec![])
    }

    /// Moves every block that plays `from` — in the open project and in the
    /// rig's presets — to the same capture in `to`. Only the open project's
    /// blocks are reported; presets are read again when they are loaded.
    pub(crate) fn follow_plugin_blocks(
        &self,
        from: &PluginManifest,
        to: &PluginManifest,
    ) -> Vec<Event> {
        let path = capture_grid(to)
            .and_then(|(parameters, _)| parameters.first())
            .map(|p| p.name.clone())
            .unwrap_or_default();
        let mut events = Vec::new();
        {
            let mut project = self.project.borrow_mut();
            for chain in &mut project.chains {
                for block in follow_blocks(&mut chain.blocks, from, to) {
                    events.push(Event::BlockParameterChanged {
                        chain: chain.id.clone(),
                        block,
                        path: path.clone(),
                    });
                }
            }
        }
        if let Some(rig) = self.rig.borrow().as_ref() {
            for preset in rig.borrow_mut().presets.values_mut() {
                follow_blocks(&mut preset.blocks, from, to);
            }
        }
        events
    }
}
