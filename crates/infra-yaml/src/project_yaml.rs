//! Responsibility: maps the legacy `project.yaml` document onto the project model.
//!
//! Split out of `lib.rs` (#873). The current format is `project.openrig`
//! ([`crate::rig_yaml`]); this is the shape that came before it.

use anyhow::Result;
use serde::{Deserialize, Serialize};
use std::fs;
use std::path::PathBuf;

use anyhow::Context;
use project::project::Project;

use crate::chain_yaml::ChainYaml;
use crate::device_yaml::DeviceSettingsYaml;

pub struct YamlProjectRepository {
    pub path: PathBuf,
}

pub fn serialize_project(project: &Project) -> Result<String> {
    let dto = ProjectYaml::from_project(project)?;
    Ok(serde_yaml::to_string(&dto)?)
}

impl YamlProjectRepository {
    pub fn load_current_project(&self) -> Result<Project> {
        log::info!("loading project from {:?}", self.path);
        let raw = fs::read_to_string(&self.path)
            .with_context(|| format!("failed to read yaml {:?}", self.path))?;
        let dto: ProjectYaml = serde_yaml::from_str(&raw)?;
        let project = dto.into_project()?;
        log::debug!("project loaded: {} chains", project.chains.len());
        Ok(project)
    }

    pub fn save_project(&self, project: &Project) -> Result<()> {
        log::info!("saving project to {:?}", self.path);
        let dto = ProjectYaml::from_project(project)?;
        if let Some(parent) = self.path.parent() {
            fs::create_dir_all(parent)?;
        }
        fs::write(&self.path, serde_yaml::to_string(&dto)?)?;
        log::debug!("project saved: {} chains", project.chains.len());
        Ok(())
    }
}

#[derive(Debug, Deserialize, Serialize)]
pub(crate) struct ProjectYaml {
    #[serde(default)]
    name: Option<String>,
    #[serde(default, skip_serializing)]
    device_settings: Vec<DeviceSettingsYaml>,
    chains: Vec<ChainYaml>,
}

impl ProjectYaml {
    pub(crate) fn into_project(self) -> Result<Project> {
        Ok(Project {
            name: self.name,
            device_settings: self.device_settings.into_iter().map(Into::into).collect(),
            chains: self
                .chains
                .into_iter()
                .enumerate()
                .map(|(index, chain)| chain.into_chain(index))
                .collect::<Result<Vec<_>>>()?,
            // #513: legacy YAML projects predate the project-owned MIDI
            // bindings (#499 / #513) — they have no `midi:` key. New
            // projects round-trip through `RigProject` (rig.yaml), not this
            // path; the YAML adapter here only deals with the legacy
            // `Project` shape, so `None` is the correct value.
            midi: None,
        })
    }

    fn from_project(project: &Project) -> Result<Self> {
        Ok(Self {
            name: project.name.clone(),
            device_settings: Vec::new(),
            chains: project
                .chains
                .iter()
                .map(ChainYaml::from_chain)
                .collect::<Result<Vec<_>>>()?,
        })
    }
}
