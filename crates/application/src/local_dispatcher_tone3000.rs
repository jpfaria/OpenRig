//! Responsibility: handles the TONE3000 browser commands.
//!
//! The key is set and a package removed on the dispatching thread. A search
//! and an install talk to the network, so each runs on its own thread and
//! reports through `poll_async_results`, like the catalog reload (#693).

use std::cell::RefCell;
use std::path::PathBuf;
use std::rc::Rc;
use std::sync::mpsc::Sender;

use anyhow::{bail, Result};

use crate::app_config_persist::persist_tone3000;
use crate::command::{Command, Tone3000Command};
use crate::event::{Event, Tone3000Event};
use crate::local_dispatcher::{AsyncDone, LocalDispatcher};
use crate::tone3000::api_types::{Page, Tone};
use crate::tone3000::api_url::SearchQuery;
use crate::tone3000::install::{install_tone, update_tone, InstallProgress, InstallRequest};
use crate::tone3000::installed::remove_package;
use crate::tone3000_state::{Tone3000ControlState, Tone3000Snapshot};

/// Results per search page.
const SEARCH_PAGE_SIZE: u32 = 25;

/// What a TONE3000 worker thread reports back.
#[derive(Debug)]
pub(crate) enum Tone3000Done {
    Search {
        generation: u64,
        result: Result<Page<Tone>, String>,
    },
    Progress {
        tone_id: u64,
        progress: InstallProgress,
    },
    Install {
        tone_id: u64,
        result: Result<String, String>,
    },
}

impl LocalDispatcher {
    /// Adopt the TONE3000 browser state the frontend built for this session.
    pub fn attach_tone3000_state(&self, state: Rc<RefCell<Tone3000ControlState>>) {
        *self.tone3000.borrow_mut() = state;
    }

    pub(crate) fn tone3000_snapshot(&self) -> Tone3000Snapshot {
        self.tone3000_state().borrow().snapshot()
    }

    fn tone3000_state(&self) -> Rc<RefCell<Tone3000ControlState>> {
        self.tone3000.borrow().clone()
    }

    pub(crate) fn handle_tone3000(&self, cmd: Command) -> Result<Vec<Event>> {
        let Command::Tone3000(cmd) = cmd else {
            bail!("handle_tone3000 called with a non-TONE3000 command");
        };
        match cmd {
            Tone3000Command::SetTone3000ApiKey { key } => self.set_tone3000_key(key),
            Tone3000Command::SearchTone3000 {
                query,
                page,
                format,
                gear,
                sort,
            } => self.search_tone3000(SearchQuery {
                query,
                page: page.max(1),
                page_size: SEARCH_PAGE_SIZE,
                format,
                gear,
                sort,
            }),
            Tone3000Command::InstallTone3000 {
                tone_id,
                architecture,
                block_type,
            } => self.install_tone3000(
                InstallRequest {
                    tone_id,
                    architecture,
                    block_type,
                },
                false,
            ),
            Tone3000Command::UninstallTone3000 { plugin_id } => self.uninstall_tone3000(plugin_id),
            Tone3000Command::UpdateTone3000 {
                tone_id,
                architecture,
            } => self.install_tone3000(
                InstallRequest {
                    tone_id,
                    architecture,
                    block_type: None,
                },
                true,
            ),
        }
    }

    fn set_tone3000_key(&self, key: String) -> Result<Vec<Event>> {
        let key = Some(key.trim().to_string()).filter(|k| !k.is_empty());
        let state = self.tone3000_state();
        let configured = state.borrow_mut().set_key(key.clone());
        if let Some(path) = state.borrow().config_path() {
            persist_tone3000(path, move |config| config.api_key = key);
        }
        Ok(vec![Event::Tone3000(Tone3000Event::KeyChanged {
            configured,
        })])
    }

    fn search_tone3000(&self, query: SearchQuery) -> Result<Vec<Event>> {
        let state = self.tone3000_state();
        let Some(api) = state.borrow().api() else {
            bail!("set a TONE3000 Secret Key before searching");
        };
        let generation = state.borrow_mut().start_search(query.clone());
        let tx = self.async_done_tx.clone();
        std::thread::Builder::new()
            .name("tone3000-search".into())
            .spawn(move || {
                let result = api.search(&query).map_err(|e| e.to_string());
                let _ = tx.send(AsyncDone::Tone3000(Tone3000Done::Search {
                    generation,
                    result,
                }));
            })
            .map_err(|e| anyhow::anyhow!("failed to spawn tone3000-search task: {e}"))?;
        Ok(vec![])
    }

    /// Installs `request` on a worker thread; `replace` updates a package
    /// that is already there.
    fn install_tone3000(&self, request: InstallRequest, replace: bool) -> Result<Vec<Event>> {
        let state = self.tone3000_state();
        let (api, root) = {
            let state = state.borrow();
            match (state.api(), state.root()) {
                (Some(api), Some(root)) => (api, root),
                (None, _) => bail!("set a TONE3000 Secret Key before installing"),
                (_, None) => bail!("no TONE3000 plugin folder is attached"),
            }
        };
        let tone_id = request.tone_id;
        if state.borrow().is_installing(tone_id) {
            bail!("TONE3000 tone {tone_id} is already installing");
        }
        state.borrow_mut().start_install(tone_id);
        let tx = self.async_done_tx.clone();
        std::thread::Builder::new()
            .name("tone3000-install".into())
            .spawn(move || {
                let result = run_install(api.as_ref(), root, &request, replace, &tx);
                let _ = tx.send(AsyncDone::Tone3000(Tone3000Done::Install {
                    tone_id,
                    result,
                }));
            })
            .map_err(|e| anyhow::anyhow!("failed to spawn tone3000-install task: {e}"))?;
        Ok(vec![])
    }

    /// Removes a listed TONE3000 plugin that sits in the plugins folder,
    /// whether the browser installed it or the user put it there.
    fn uninstall_tone3000(&self, plugin_id: String) -> Result<Vec<Event>> {
        let state = self.tone3000_state();
        let Some(root) = state.borrow().root() else {
            bail!("no plugins folder is attached");
        };
        let listed = state.borrow().snapshot().installed;
        let Some(entry) = listed.into_iter().find(|e| e.plugin_id == plugin_id) else {
            bail!("`{plugin_id}` is not an installed TONE3000 plugin");
        };
        if !entry.removable {
            bail!("`{plugin_id}` is not in the plugins folder");
        }
        remove_package(&root, &entry.dir)?;
        // Not in the catalog (never loaded) is fine: the folder is gone.
        let _ = plugin_loader::registry::unload(&plugin_id);
        state.borrow_mut().refresh_installed();
        Ok(vec![Event::Tone3000(Tone3000Event::Uninstalled {
            plugin_id,
        })])
    }

    /// Applies one worker report to the state and names what changed.
    pub(crate) fn finish_tone3000(&self, done: Tone3000Done) -> Vec<Event> {
        let state = self.tone3000_state();
        let mut state = state.borrow_mut();
        let event = match done {
            Tone3000Done::Search { generation, result } => {
                let event = match &result {
                    Ok(page) => Tone3000Event::SearchFinished { total: page.total },
                    Err(message) => Tone3000Event::SearchFailed {
                        message: message.clone(),
                    },
                };
                if !state.finish_search(generation, result) {
                    return Vec::new();
                }
                event
            }
            Tone3000Done::Progress { tone_id, progress } => {
                state.set_progress(tone_id, progress.clone());
                Tone3000Event::InstallProgress { tone_id, progress }
            }
            Tone3000Done::Install { tone_id, result } => match result {
                Ok(plugin_id) => {
                    state.finish_install(tone_id, Ok(()));
                    Tone3000Event::Installed { tone_id, plugin_id }
                }
                Err(message) => {
                    state.finish_install(tone_id, Err(message.clone()));
                    Tone3000Event::InstallFailed { tone_id, message }
                }
            },
        };
        vec![Event::Tone3000(event)]
    }
}

/// Installs, then brings the new package into the catalog so a block can
/// pick it right away. An update drops the old catalog entry first, since
/// the catalog keeps a loaded id as it is. Runs on the worker thread.
fn run_install(
    api: &dyn crate::tone3000::api_client::Tone3000Api,
    root: PathBuf,
    request: &InstallRequest,
    replace: bool,
    tx: &Sender<AsyncDone>,
) -> Result<String, String> {
    let tone_id = request.tone_id;
    let mut report = |progress: InstallProgress| {
        let _ = tx.send(AsyncDone::Tone3000(Tone3000Done::Progress {
            tone_id,
            progress,
        }));
    };
    let installed = if replace {
        update_tone(api, &root, request, &mut report)
    } else {
        install_tone(api, &root, request, &mut report)
    }
    .map_err(|e| e.to_string())?;
    if replace {
        let _ = plugin_loader::registry::unload(&installed.plugin_id);
    }
    plugin_loader::registry::load_one(&installed.plugin_id, &[installed.dir])
        .map_err(|e| format!("installed, but the catalog did not load it: {e}"))?;
    Ok(installed.plugin_id)
}
