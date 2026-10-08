//! Responsibility: holds the TONE3000 browser's control-plane state.
//!
//! The dispatcher owns it, so the screen and an MCP client see the same key
//! status, search, installs in flight and installed packages. The key stays
//! inside: the snapshot only says whether one is set (#879).

use std::path::PathBuf;
use std::sync::Arc;

use infra_filesystem::Tone3000Config;
use plugin_loader::manifest::{BlockType, NamArchitecture};
use serde::Serialize;

use crate::tone3000::api_client::Tone3000Api;
use crate::tone3000::api_http::HttpTone3000Api;
use crate::tone3000::api_types::{Page, Tone};
use crate::tone3000::api_url::SearchQuery;
use crate::tone3000::catalog_tones::{installed_entry, loaded_catalog_entries};
use crate::tone3000::catalog_tones_cache::GenerationCache;
use crate::tone3000::install::InstallProgress;
use crate::tone3000::install_pending::PendingInstall;
use crate::tone3000::installed::list_installed;

/// Builds the API client for a key. Tests swap in a fake.
pub type Tone3000ApiFactory = Arc<dyn Fn(&str) -> Arc<dyn Tone3000Api> + Send + Sync>;

/// The catalog plugins that came from TONE3000, other than the browser's own.
/// Read on every snapshot, so a catalog reload shows at once; walked again
/// only when the catalog's generation moves. Tests swap it.
pub type Tone3000CatalogSource = Arc<dyn Fn() -> Vec<Tone3000InstalledEntry> + Send + Sync>;

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

/// One plugin the user has from a TONE3000 tone.
#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct Tone3000InstalledEntry {
    pub plugin_id: String,
    /// The TONE3000 tones it came from.
    pub tone_ids: Vec<u64>,
    pub display_name: String,
    pub block_type: BlockType,
    pub architecture: Option<NamArchitecture>,
    pub captures: usize,
    /// It sits in the plugins folder, which is the user's, so the browser
    /// may remove it. A plugin from anywhere else is only listed.
    pub removable: bool,
    /// The TONE3000 tone version the browser installed; `None` when unknown.
    pub updated_at: Option<String>,
    /// The package folder.
    pub dir: PathBuf,
}

/// The browser state as a value. Holds no key.
#[derive(Debug, Clone, Default, PartialEq, Serialize)]
pub struct Tone3000Snapshot {
    pub key_configured: bool,
    /// A key is set and there is a folder to install into.
    pub can_install: bool,
    pub search: Tone3000SearchSnapshot,
    pub installs: Vec<Tone3000InstallEntry>,
    /// The browser's packages first, then the catalog's.
    pub installed: Vec<Tone3000InstalledEntry>,
    /// Downloads waiting for the user to name their parameters.
    pub naming: Vec<PendingInstall>,
}

pub struct Tone3000ControlState {
    api_key: Option<String>,
    /// The per-machine `config.yaml`; `None` persists nothing, which keeps
    /// tests off the user's real config.
    config_path: Option<PathBuf>,
    /// The plugins folder installs go into; `None` means installs are off.
    root: Option<PathBuf>,
    api_factory: Tone3000ApiFactory,
    catalog: Tone3000CatalogSource,
    search: Tone3000SearchSnapshot,
    /// Bumped on every search, so a slow answer to an older one is dropped.
    search_generation: u64,
    installs: Vec<Tone3000InstallEntry>,
    installed: Vec<Tone3000InstalledEntry>,
    naming: Vec<PendingInstall>,
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
        let plugins_root = root.clone();
        let mut state = Self {
            api_key: config.api_key.clone().filter(|k| !k.trim().is_empty()),
            config_path,
            root,
            api_factory: Arc::new(|key: &str| -> Arc<dyn Tone3000Api> {
                Arc::new(HttpTone3000Api::new(key))
            }),
            catalog: {
                let cache = GenerationCache::default();
                Arc::new(move || {
                    cache.get(plugin_loader::registry::generation(), || {
                        loaded_catalog_entries(plugins_root.as_deref())
                    })
                })
            },
            search: Tone3000SearchSnapshot::default(),
            search_generation: 0,
            installs: Vec::new(),
            installed: Vec::new(),
            naming: Vec::new(),
        };
        state.refresh_installed();
        state
    }

    pub fn with_api_factory(mut self, factory: Tone3000ApiFactory) -> Self {
        self.api_factory = factory;
        self
    }

    pub fn with_catalog(mut self, catalog: Tone3000CatalogSource) -> Self {
        self.catalog = catalog;
        self
    }

    pub fn snapshot(&self) -> Tone3000Snapshot {
        Tone3000Snapshot {
            key_configured: self.api_key.is_some(),
            can_install: self.api_key.is_some() && self.root.is_some(),
            search: self.search.clone(),
            installs: self.installs.clone(),
            installed: self
                .installed
                .iter()
                .cloned()
                .chain((self.catalog)())
                .collect(),
            naming: self.naming.clone(),
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

    /// The download of `tone_id` waiting for names, if any.
    pub fn pending(&self, tone_id: u64) -> Option<&PendingInstall> {
        self.naming.iter().find(|p| p.tone_id == tone_id)
    }

    /// Ends an install that set its download aside for names.
    pub fn wait_for_names(&mut self, pending: PendingInstall) {
        self.installs.retain(|i| i.tone_id != pending.tone_id);
        self.naming.retain(|p| p.tone_id != pending.tone_id);
        self.naming.push(pending);
    }

    /// Forgets the download of `tone_id` once it was named or dropped.
    pub fn named(&mut self, tone_id: u64) {
        self.naming.retain(|p| p.tone_id != tone_id);
    }

    /// Re-reads the packages on disk.
    pub fn refresh_installed(&mut self) {
        self.installed = self
            .root
            .as_deref()
            .map(list_installed)
            .unwrap_or_default()
            .iter()
            .map(|plugin| Tone3000InstalledEntry {
                tone_ids: tone_id_of(&plugin.plugin_id).into_iter().collect(),
                updated_at: plugin.updated_at.clone(),
                ..installed_entry(&plugin.manifest, &plugin.dir, true)
            })
            .collect();
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
