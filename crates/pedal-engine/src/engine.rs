use pedal-setup::setup::Setup;
use pedal-state::engine_state::EngineState;
use pedal-state::pedalboard_state::PedalboardState;

pub struct PedalboardEngine {
    pub setup: Setup,
    pub state: PedalboardState,
    pub engine_state: EngineState,
}

impl PedalboardEngine {
    pub fn new(setup: Setup, state: PedalboardState) -> Self {
        Self {
            setup,
            state,
            engine_state: EngineState::default(),
        }
    }

    pub fn start(&mut self) {
        self.engine_state.is_running = true;
        self.engine_state.active_tracks = self.setup.tracks.len();
    }

    pub fn stop(&mut self) {
        self.engine_state.is_running = false;
    }
}
