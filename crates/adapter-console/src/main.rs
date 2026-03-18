use anyhow::Result;
use cpal::traits::StreamTrait;
use application::validate::validate_setup;
use engine::engine::PedalboardEngine;
use infra_cpal::{build_streams_for_setup, list_devices};
use infra_yaml::{YamlSetupRepository, YamlStateRepository};
use ports::{SetupRepository, StateRepository};
use std::path::PathBuf;
use std::thread;
use std::time::Duration;
fn main() -> Result<()> {
    let setup_repo = YamlSetupRepository {
        path: PathBuf::from("setup.yaml"),
    };
    let state_repo = YamlStateRepository {
        path: PathBuf::from("state.yaml"),
    };
    let setup = setup_repo.load_current_setup()?;
    validate_setup(&setup)?;
    let state = state_repo.load_state()?;
    println!("=== Devices ===");
    for line in list_devices()? {
        println!("{line}");
    }
    println!("=== Setup ===");
    println!("setup={} tracks={}", setup.name, setup.tracks.len());
    let mut engine = PedalboardEngine::new(setup, state)?;
    let streams = build_streams_for_setup(&engine.setup, &engine)?;
    for stream in &streams {
        stream.play()?;
    }
    engine.start();
    println!("=== Engine ===");
    println!("running={} active_tracks={}", engine.engine_state.is_running, engine.engine_state.active_tracks);
    loop {
        thread::sleep(Duration::from_secs(1));
    }
}
