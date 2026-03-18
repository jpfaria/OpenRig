use anyhow::Result;
use pedal-preset::Preset;
use pedal-setup::setup::Setup;
use pedal-state::pedalboard_state::PedalboardState;

pub trait SetupRepository: Send + Sync {
    fn load_current_setup(&self) -> Result<Setup>;
    fn save_setup(&self, setup: &Setup) -> Result<()>;
}

pub trait PresetRepository: Send + Sync {
    fn load_preset(&self, preset_id: &str) -> Result<Preset>;
    fn save_preset(&self, preset: &Preset) -> Result<()>;
}

pub trait StateRepository: Send + Sync {
    fn load_state(&self) -> Result<PedalboardState>;
    fn save_state(&self, state: &PedalboardState) -> Result<()>;
}

pub trait StateSyncPort: Send + Sync {
    fn publish_state(&self, state: &PedalboardState) -> Result<()>;
}
