use anyhow::Result;
use application::validate::validate_project;
use cpal::traits::StreamTrait;
use engine::runtime::build_runtime_graph;
use infra_cpal::{build_streams_for_project, list_devices, resolve_project_chain_sample_rates};
use infra_yaml::YamlProjectRepository;
use serde::Deserialize;
use std::env;
use std::path::PathBuf;
use std::thread;
use std::time::Duration;

#[derive(Debug, Deserialize, Default)]
struct AppConfigYaml {
    #[serde(default, rename = "presets_path")]
    _presets_path: Option<PathBuf>,
}

fn main() -> Result<()> {
    let project_path = parse_project_path();
    let config_path = parse_config_path();
    let _config = load_app_config(&config_path)?;
    infra_filesystem::init_asset_paths(infra_filesystem::AssetPaths::default());
    let project_repo = YamlProjectRepository { path: project_path };
    let project = project_repo.load_current_project()?;
    validate_project(&project)?;
    println!("=== Devices ===");
    for line in list_devices()? {
        println!("{line}");
    }
    println!("=== Project ===");
    println!("chains={}", project.chains.len());
    let chain_sample_rates = resolve_project_chain_sample_rates(&project)?;
    let runtime_graph = build_runtime_graph(&project, &chain_sample_rates)?;
    let streams = build_streams_for_project(&project, &runtime_graph)?;
    for stream in &streams {
        stream.play()?;
    }
    println!("=== Engine ===");
    println!(
        "running=true active_chains={}",
        project.chains.iter().filter(|chain| chain.enabled).count()
    );
    loop {
        thread::sleep(Duration::from_secs(1));
    }
}

fn parse_project_path() -> PathBuf {
    let mut args = env::args().skip(1);
    while let Some(arg) = args.next() {
        if arg == "--project" {
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
