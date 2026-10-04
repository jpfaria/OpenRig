//! Responsibility: holds the drum machine's control-plane state.
//!
//! The dispatcher owns it, so a footswitch, an MCP client and the screen all
//! drive the same drums. The library is the set of kits and grooves installed
//! on this machine; a choice is always one of them.

use std::path::PathBuf;
use std::sync::Arc;

use feature_dsp::drums::{DrumSettings, Groove, MAX_BPM, MIN_BPM};
use feature_dsp::metronome::BPM_DEFAULT;
use infra_filesystem::DrumsConfig;

use crate::drums::{DrumKitEntry, DrumLibrary};

/// What the drums are set to, as a value a frontend renders.
#[derive(Debug, Clone, PartialEq)]
pub struct DrumsSnapshot {
    /// Whether the output is open. Not persisted.
    pub enabled: bool,
    /// Whether the groove is running. Not persisted.
    pub playing: bool,
    pub bpm: f32,
    pub volume: f32,
    pub kit: Option<String>,
    pub groove: Option<String>,
    /// The chosen output endpoint key, `None` for the project's first.
    pub output_key: Option<String>,
}

impl Default for DrumsSnapshot {
    fn default() -> Self {
        let config = DrumsConfig::default();
        Self {
            enabled: false,
            playing: false,
            bpm: BPM_DEFAULT,
            volume: config.volume,
            kit: None,
            groove: None,
            output_key: None,
        }
    }
}

#[derive(Debug, Default)]
pub struct DrumsControlState {
    snapshot: DrumsSnapshot,
    library: DrumLibrary,
    /// The per-machine `config.yaml` these settings live in; `None` persists
    /// nothing, which keeps tests off the user's real config.
    config_path: Option<PathBuf>,
}

impl DrumsControlState {
    /// The state at boot: the saved settings, stopped. A saved kit or groove
    /// that is no longer installed falls back to the first one.
    pub fn restored(
        config: &DrumsConfig,
        library: DrumLibrary,
        config_path: Option<PathBuf>,
    ) -> Self {
        let kit = pick(&library.kits, config.kit.as_deref(), |k| &k.id);
        let groove = pick(&library.grooves, config.groove.as_deref(), |g| &g.id);
        let snapshot = DrumsSnapshot {
            enabled: false,
            playing: false,
            bpm: BPM_DEFAULT,
            volume: clamp_volume(config.volume),
            kit,
            groove,
            output_key: config.output_device.clone(),
        };
        Self {
            snapshot,
            library,
            config_path,
        }
    }

    pub fn snapshot(&self) -> DrumsSnapshot {
        self.snapshot.clone()
    }

    pub fn library(&self) -> &DrumLibrary {
        &self.library
    }

    pub fn config_path(&self) -> Option<PathBuf> {
        self.config_path.clone()
    }

    pub fn settings(&self) -> DrumSettings {
        DrumSettings {
            bpm: self.snapshot.bpm,
            volume: self.snapshot.volume,
        }
    }

    pub fn kit_entry(&self) -> Option<&DrumKitEntry> {
        let id = self.snapshot.kit.as_deref()?;
        self.library.kits.iter().find(|k| k.id == id)
    }

    pub fn groove(&self) -> Option<Arc<Groove>> {
        let id = self.snapshot.groove.as_deref()?;
        self.find_groove(id)
    }

    pub fn find_groove(&self, id: &str) -> Option<Arc<Groove>> {
        self.library
            .grooves
            .iter()
            .find(|g| g.id == id)
            .map(|g| Arc::new(g.clone()))
    }

    pub fn find_kit(&self, id: &str) -> Option<&DrumKitEntry> {
        self.library.kits.iter().find(|k| k.id == id)
    }

    pub fn output_key(&self) -> Option<&str> {
        self.snapshot.output_key.as_deref()
    }

    pub fn set_transport(&mut self, enabled: bool, playing: bool) {
        self.snapshot.enabled = enabled;
        self.snapshot.playing = playing;
    }

    /// Clamps and stores the tempo; returns what was stored.
    pub fn set_bpm(&mut self, bpm: f32) -> f32 {
        self.snapshot.bpm = clamp_bpm(bpm);
        self.snapshot.bpm
    }

    /// Clamps and stores the level; returns what was stored.
    pub fn set_volume(&mut self, volume: f32) -> f32 {
        self.snapshot.volume = clamp_volume(volume);
        self.snapshot.volume
    }

    pub fn set_kit(&mut self, id: String) {
        self.snapshot.kit = Some(id);
    }

    pub fn set_groove(&mut self, id: String) {
        self.snapshot.groove = Some(id);
    }

    pub fn set_output_key(&mut self, key: Option<String>) {
        self.snapshot.output_key = key;
    }
}

/// The saved id if it is still installed, else the first item's id.
fn pick<T>(items: &[T], saved: Option<&str>, id: impl Fn(&T) -> &String) -> Option<String> {
    saved
        .and_then(|saved| items.iter().find(|item| id(item) == saved))
        .or_else(|| items.first())
        .map(|item| id(item).clone())
}

fn clamp_bpm(bpm: f32) -> f32 {
    if bpm.is_finite() {
        bpm.clamp(MIN_BPM, MAX_BPM)
    } else {
        BPM_DEFAULT
    }
}

fn clamp_volume(volume: f32) -> f32 {
    if volume.is_finite() {
        volume.clamp(0.0, 1.0)
    } else {
        0.0
    }
}
