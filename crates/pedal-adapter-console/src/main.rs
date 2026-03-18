use anyhow::Result;
use pedal-application::validate::validate_setup;
use pedal-engine::engine::PedalboardEngine;
use pedal-infra-cpal::list_devices;
use pedal-infra-yaml::{YamlSetupRepository, YamlStateRepository};
use pedal-ports::{SetupRepository, StateRepository};
use std::path::PathBuf;

fn main() -> Result<()> {
    let setup_repo = YamlSetupRepository { path: PathBuf::from("setup.yaml") };
    let state_repo = YamlStateRepository { path: PathBuf::from("state.yaml") };

    let setup = setup_repo.load_current_setup()?;
    validate_setup(&setup)?;
    let state = state_repo.load_state()?;

    println!("=== Devices ===");
    for line in list_devices()? {
        println!("{line}");
    }

    println!("=== Setup ===");
    println!("tracks={}", setup.tracks.len());

    let mut engine = PedalboardEngine::new(setup, state);
    engine.start();

    println!("engine_running={}", engine.engine_state.is_running);
    Ok(())
}
