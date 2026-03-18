use anyhow::Result;
use domain::ids::TrackId;
use setup::setup::Setup;
use state::engine_state::EngineState;
use state::pedalboard_state::PedalboardState;
use std::sync::{Arc, Mutex};
use crate::runtime::{build_runtime_graph, RuntimeGraph, TrackRuntimeState};
pub struct PedalboardEngine {
    pub setup: Setup,
    pub state: PedalboardState,
    pub engine_state: EngineState,
    pub runtime_graph: RuntimeGraph,
}
impl PedalboardEngine {
    pub fn new(setup: Setup, state: PedalboardState) -> Result<Self> {
        let runtime_graph = build_runtime_graph(&setup)?;
        Ok(Self {
            setup,
            state,
            engine_state: EngineState::default(),
            runtime_graph,
        })
    }
    pub fn start(&mut self) {
        self.engine_state.is_running = true;
        self.engine_state.active_tracks = self.setup.tracks.len();
    }
    pub fn stop(&mut self) {
        self.engine_state.is_running = false;
    }
    pub fn runtime_for_track(&self, track_id: &TrackId) -> Option<Arc<Mutex<TrackRuntimeState>>> {
        self.runtime_graph.tracks.get(track_id).cloned()
    }
}
