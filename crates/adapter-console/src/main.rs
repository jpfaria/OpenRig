use anyhow::Result;
use application::validate::validate_setup;
use cpal::traits::StreamTrait;
use engine::engine::PedalboardEngine;
use infra_cpal::{build_streams_for_setup, list_devices};
use infra_yaml::YamlSetupRepository;
use ports::SetupRepository;
use serde::Deserialize;
use std::env;
use std::path::PathBuf;
use std::thread;
use std::time::Duration;
use state::pedalboard_state::PedalboardState;

#[derive(Debug, Deserialize, Default)]
struct AppConfigYaml {
    #[serde(default)]
    presets_path: Option<PathBuf>,
}

fn main() -> Result<()> {
    let setup_path = parse_setup_path();
    let config_path = parse_config_path();
    let config = load_app_config(&config_path)?;
    let setup_repo = YamlSetupRepository {
        path: setup_path,
        presets_path_override: config.presets_path,
    };
    let setup = setup_repo.load_current_setup()?;
    validate_setup(&setup)?;
    let state = PedalboardState::default();
    println!("=== Devices ===");
    for line in list_devices()? {
        println!("{line}");
    }
    println!("=== Setup ===");
    println!(
        "presets={} tracks={}",
        setup.presets.len(),
        setup.tracks.len()
    );
    let mut engine = PedalboardEngine::new(setup, state)?;
    let streams = build_streams_for_setup(&engine.setup, &engine)?;
    for stream in &streams {
        stream.play()?;
    }
    engine.start();
    println!("=== Engine ===");
    println!(
        "running={} active_tracks={}",
        engine.engine_state.is_running, engine.engine_state.active_tracks
    );
    loop {
        thread::sleep(Duration::from_secs(1));
    }
}

fn parse_setup_path() -> PathBuf {
    let mut args = env::args().skip(1);
    while let Some(arg) = args.next() {
        if arg == "--setup" {
            if let Some(path) = args.next() {
                return PathBuf::from(path);
            }
        }
    }
    let local_project = PathBuf::from("project.yaml");
    if local_project.exists() {
        return local_project;
    }

    PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../../project.yaml")
}

fn parse_config_path() -> PathBuf {
    let mut args = env::args().skip(1);
    while let Some(arg) = args.next() {
        if arg == "--config" {
            if let Some(path) = args.next() {
                return PathBuf::from(path);
            }
        }
    }
    let local_config = PathBuf::from("config.yaml");
    if local_config.exists() {
        return local_config;
    }

    PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../../config.yaml")
}

fn load_app_config(path: &PathBuf) -> Result<AppConfigYaml> {
    if !path.exists() {
        return Ok(AppConfigYaml::default());
    }
    let raw = std::fs::read_to_string(path)?;
    Ok(serde_yaml::from_str(&raw)?)
}
