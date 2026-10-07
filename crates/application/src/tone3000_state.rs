//! Responsibility: holds the TONE3000 browser's control-plane state.
//!
//! The dispatcher owns it, so the screen and an MCP client see the same key
//! status, search, installs in flight and installed packages. The key stays
//! inside: the snapshot only says whether one is set (#879).

use std::path::PathBuf;
use std::sync::Arc;

use infra_filesystem::Tone3000Config;
use plugin_loader::manifest::{Backend, BlockType, NamArchitecture};
use serde::Serialize;

use crate::tone3000::api_client::Tone3000Api;
use crate::tone3000::api_http::HttpTone3000Api;
use crate::tone3000::api_types::{Page, Tone};
use crate::tone3000::api_url::SearchQuery;
use crate::tone3000::install::InstallProgress;
use crate::tone3000::installed::{list_installed, InstalledPlugin};

/// Builds the API client for a key. Tests swap in a fake.
pub type Tone3000ApiFactory = Arc<dyn Fn(&str) -> Arc<dyn Tone3000Api> + Send + Sync>;

/// The last search, as a frontend renders it.
#[derive(Debug, Clone, Default, PartialEq, Serialize)]
pub struct Tone3000SearchSnapshot {
    pub query: Option<SearchQuery>,
    pub in_flight: bool,
    pub results: Option<Page<Tone>>,
    pub error: Option<String>,
}

/// One install that is running, or that failed and is still on screen.
#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct Tone3000InstallEntry {
    pub tone_id: u64,
    pub progress: InstallProgress,
    pub error: Option<String>,
}

/// One TONE3000 package on disk.
#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct Tone3000InstalledEntry {
    pub plugin_id: String,
    pub tone_id: Option<u64>,
    pub display_name: String,
    pub block_type: BlockType,
    pub architecture: Option<NamArchitecture>,
    pub captures: usize,
}

/// The browser state as a value. Holds no key.
#[derive(Debug, Clone, Default, PartialEq, Serialize)]
pub struct Tone3000Snapshot {
    pub key_configured: bool,
    /// A key is set and there is a folder to install into.
    pub can_install: bool,
    pub search: Tone3000SearchSnapshot,
    pub installs: Vec<Tone3000InstallEntry>,
    pub installed: Vec<Tone3000InstalledEntry>,
}

pub struct Tone3000ControlState {
    api_key: Option<String>,
    /// The per-machine `config.yaml`; `None` persists nothing, which keeps
    /// tests off the user's real config.
    config_path: Option<PathBuf>,
    /// The TONE3000 plugin root; `None` means installs are off.
    root: Option<PathBuf>,
    api_factory: Tone3000ApiFactory,
    search: Tone3000SearchSnapshot,
    /// Bumped on every search, so a slow answer to an older one is dropped.
    search_generation: u64,
    installs: Vec<Tone3000InstallEntry>,
    installed: Vec<Tone3000InstalledEntry>,
}

impl Default for Tone3000ControlState {
    fn default() -> Self {
        Self::restored(&Tone3000Config::default(), None, None)
    }
}

impl std::fmt::Debug for Tone3000ControlState {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("Tone3000ControlState")
            .field("snapshot", &self.snapshot())
            .finish()
    }
}

impl Tone3000ControlState {
    /// The state at boot: the saved key and what is already installed.
    pub fn restored(
        config: &Tone3000Config,
        config_path: Option<PathBuf>,
        root: Option<PathBuf>,
    ) -> Self {
        let mut state = Self {
            api_key: config.api_key.clone().filter(|k| !k.trim().is_empty()),
            config_path,
            root,
            api_factory: Arc::new(|key: &str| -> Arc<dyn Tone3000Api> {
                Arc::new(HttpTone3000Api::new(key))
            }),
            search: Tone3000SearchSnapshot::default(),
            search_generation: 0,
            installs: Vec::new(),
            installed: Vec::new(),
        };
        state.refresh_installed();
        state
    }

    pub fn with_api_factory(mut self, factory: Tone3000ApiFactory) -> Self {
        self.api_factory = factory;
        self
    }

    pub fn snapshot(&self) -> Tone3000Snapshot {
        Tone3000Snapshot {
            key_configured: self.api_key.is_some(),
            can_install: self.api_key.is_some() && self.root.is_some(),
            search: self.search.clone(),
            installs: self.installs.clone(),
            installed: self.installed.clone(),
        }
    }

    pub fn config_path(&self) -> Option<PathBuf> {
        self.config_path.clone()
    }

    pub fn root(&self) -> Option<PathBuf> {
        self.root.clone()
    }

    /// Sets or clears the key; returns whether one is set now.
    pub fn set_key(&mut self, key: Option<String>) -> bool {
        self.api_key = key;
        self.api_key.is_some()
    }

    /// A client for the saved key, or `None` without one.
    pub fn api(&self) -> Option<Arc<dyn Tone3000Api>> {
        self.api_key.as_deref().map(|key| (self.api_factory)(key))
    }

    /// Records a new search and returns its generation.
    pub fn start_search(&mut self, query: SearchQuery) -> u64 {
        self.search_generation += 1;
        self.search = Tone3000SearchSnapshot {
            query: Some(query),
            in_flight: true,
            results: None,
            error: None,
        };
        self.search_generation
    }

    /// Stores a search answer; `false` when a newer search replaced it.
    pub fn finish_search(&mut self, generation: u64, result: Result<Page<Tone>, String>) -> bool {
        if generation != self.search_generation {
            return false;
        }
        self.search.in_flight = false;
        match result {
            Ok(page) => self.search.results = Some(page),
            Err(message) => self.search.error = Some(message),
        }
        true
    }

    /// True while an install of this tone runs.
    pub fn is_installing(&self, tone_id: u64) -> bool {
        self.installs
            .iter()
            .any(|i| i.tone_id == tone_id && i.error.is_none())
    }

    /// Starts an install entry, replacing a failed one of the same tone.
    pub fn start_install(&mut self, tone_id: u64) {
        self.installs.retain(|i| i.tone_id != tone_id);
        self.installs.push(Tone3000InstallEntry {
            tone_id,
            progress: InstallProgress::Fetching,
            error: None,
        });
    }

    pub fn set_progress(&mut self, tone_id: u64, progress: InstallProgress) {
        if let Some(entry) = self.installs.iter_mut().find(|i| i.tone_id == tone_id) {
            entry.progress = progress;
        }
    }

    pub fn finish_install(&mut self, tone_id: u64, result: Result<(), String>) {
        match result {
            Ok(()) => self.installs.retain(|i| i.tone_id != tone_id),
            Err(message) => {
                if let Some(entry) = self.installs.iter_mut().find(|i| i.tone_id == tone_id) {
                    entry.error = Some(message);
                }
            }
        }
        self.refresh_installed();
    }

    /// Re-reads the packages on disk.
    pub fn refresh_installed(&mut self) {
        self.installed = self
            .root
            .as_deref()
            .map(list_installed)
            .unwrap_or_default()
            .iter()
            .map(installed_entry)
            .collect();
    }
}

fn installed_entry(plugin: &InstalledPlugin) -> Tone3000InstalledEntry {
    let manifest = &plugin.manifest;
    let captures = match &manifest.backend {
        Backend::Nam { captures, .. } | Backend::Ir { captures, .. } => captures.len(),
        _ => 0,
    };
    Tone3000InstalledEntry {
        plugin_id: plugin.plugin_id.clone(),
        tone_id: tone_id_of(&plugin.plugin_id),
        display_name: manifest.display_name.clone(),
        block_type: manifest.block_type,
        architecture: manifest.architecture,
        captures,
    }
}

/// `tone3000_<id>` or `tone3000_<id>_a<n>` → `<id>`.
fn tone_id_of(plugin_id: &str) -> Option<u64> {
    plugin_id
        .strip_prefix("tone3000_")?
        .split('_')
        .next()?
        .parse()
        .ok()
}
