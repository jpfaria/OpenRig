//! Responsibility: holds the global mixer's control-plane state.
//! #1007: every strip's fader and mute, keyed by the strip's wire id, plus the
//! per-machine `config.yaml` they persist to.

use std::collections::BTreeMap;
use std::path::PathBuf;

use domain::mixer_gain::clamp_gain_db;
use domain::mixer_strip::MixerStripId;
use infra_filesystem::MixerStripConfig;

/// One strip's setting. The default is unity and unmuted.
#[derive(Debug, Clone, Copy, PartialEq, Default)]
pub struct MixerStripSetting {
    pub gain_db: f32,
    pub muted: bool,
}

/// The dispatcher's mixer state.
#[derive(Debug, Default)]
pub struct MixerControlState {
    settings: BTreeMap<String, MixerStripSetting>,
    /// `None` ⇒ nothing is persisted (every test, a headless transport) — the
    /// #701 guard, same as the metronome.
    config_path: Option<PathBuf>,
}

impl MixerControlState {
    /// The state a frontend restores at boot. Entries whose id does not parse
    /// (a hand-edited file) are dropped rather than failing the boot; the
    /// fader is re-clamped.
    pub fn restored(config: &[MixerStripConfig], config_path: Option<PathBuf>) -> Self {
        let settings = config
            .iter()
            .filter(|s| MixerStripId::parse(&s.id).is_some())
            .map(|s| {
                (
                    s.id.clone(),
                    MixerStripSetting {
                        gain_db: clamp_gain_db(s.gain_db),
                        muted: s.muted,
                    },
                )
            })
            .collect();
        Self {
            settings,
            config_path,
        }
    }

    /// Where these settings persist, if anywhere.
    pub fn config_path(&self) -> Option<PathBuf> {
        self.config_path.clone()
    }

    /// The strip's setting; unity when it was never moved.
    pub fn get(&self, strip: &str) -> MixerStripSetting {
        self.settings.get(strip).copied().unwrap_or_default()
    }

    /// Record a strip's setting.
    pub fn set(&mut self, strip: &str, setting: MixerStripSetting) {
        self.settings.insert(strip.to_string(), setting);
    }

    /// Every strip ever moved, with its setting.
    pub fn entries(&self) -> Vec<(String, MixerStripSetting)> {
        self.settings
            .iter()
            .map(|(id, setting)| (id.clone(), *setting))
            .collect()
    }
}
