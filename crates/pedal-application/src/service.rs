use anyhow::Result;
use pedal_ports::{PresetRepository, SetupRepository, StateRepository, StateSyncPort};
use pedal_state::command::Command;
use pedal_state::event::Event;
use pedal_state::pedalboard_state::PedalboardState;

pub struct ApplicationService<S, P, T, Y>
where
    S: SetupRepository,
    P: PresetRepository,
    T: StateRepository,
    Y: StateSyncPort,
{
    pub setup_repo: S,
    pub preset_repo: P,
    pub state_repo: T,
    pub state_sync: Y,
}

impl<S, P, T, Y> ApplicationService<S, P, T, Y>
where
    S: SetupRepository,
    P: PresetRepository,
    T: StateRepository,
    Y: StateSyncPort,
{
    pub fn load_state(&self) -> Result<PedalboardState> {
        self.state_repo.load_state()
    }

    pub fn handle_command(&self, mut state: PedalboardState, command: Command) -> Result<(PedalboardState, Event)> {
        let event = match command {
            Command::LoadPreset { preset_id } => {
                let preset = self.preset_repo.load_preset(&preset_id.0)?;
                state.active_preset_id = Some(preset.id.clone());
                state.bypass_by_block = preset.bypass_by_block;
                state.selected_option_by_block = preset.selected_option_by_block;
                state.parameter_values = preset.parameter_values;
                Event::PresetLoaded { preset_id }
            }
            Command::SetBlockBypass { block_id, bypass } => {
                state.bypass_by_block.insert(block_id.clone(), bypass);
                Event::BlockBypassChanged { block_id, bypass }
            }
            Command::SetParameterValue { parameter_id, value } => {
                state.parameter_values.insert(parameter_id.clone(), value);
                Event::ParameterValueChanged { parameter_id, value }
            }
            Command::SelectBlockOption { block_id, option_block_id } => {
                state.selected_option_by_block.insert(block_id.clone(), option_block_id.clone());
                Event::BlockOptionSelected { block_id, option_block_id }
            }
            Command::SetTrackGain { track_id, value } => {
                Event::TrackGainChanged { track_id, value }
            }
        };

        self.state_repo.save_state(&state)?;
        self.state_sync.publish_state(&state)?;

        Ok((state, event))
    }
}
