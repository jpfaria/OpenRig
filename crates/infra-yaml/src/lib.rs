//! Responsibility: routes the YAML crate's public surface.

// Snapshot of complexity debt that existed on develop before the
// #548 build break was fixed (issue #576). Refactor of long fns and
// complex types is tracked under god-file ticket #276 and follow-ups.
// Allowing crate-wide keeps the QG honest about NEW regressions
// instead of perpetually re-reporting the existing snapshot.
#![allow(clippy::too_many_lines)]
#![allow(clippy::cognitive_complexity)]
#![allow(clippy::too_many_arguments)]
#![allow(clippy::type_complexity)]

mod block_yaml;
mod block_yaml_load;
mod block_yaml_save;
mod chain_yaml;
mod default_models;
mod device_yaml;
mod param_flatten;
mod preset_yaml;
mod project_yaml;
mod rig_yaml;
mod yaml_ids;

pub use rig_yaml::{
    load_project_any, load_rig_project_file, migrate_legacy_project_file, parse_rig_project,
    save_rig_project_file, serialize_rig_project,
};

pub use block_yaml_save::serialize_audio_blocks;
pub use preset_yaml::{
    load_chain_preset_file, load_legacy_preset_as_rig, save_chain_preset_file, ChainBlocksPreset,
};
pub use project_yaml::{serialize_project, YamlProjectRepository};

pub(crate) use default_models::*;
pub(crate) use param_flatten::{
    flatten_parameter_set, generated_block_id, parameter_set_to_yaml_value,
};
pub(crate) use yaml_ids::{generated_chain_id, generated_preset_chain_id};

// The test modules are attached at the crate root and address these through
// `super::`, which is where they lived before the split (#873).
#[cfg(test)]
pub(crate) use block_yaml::AudioBlockYaml;
#[cfg(test)]
pub(crate) use domain::ids::DeviceId;
#[cfg(test)]
pub(crate) use param_flatten::{
    insert_yaml_value, yaml_key_to_string, yaml_scalar_to_parameter_value,
};
#[cfg(test)]
pub(crate) use project_yaml::ProjectYaml;

#[cfg(test)]
#[path = "lib_tests.rs"]
mod tests;

#[cfg(test)]
#[path = "lib_roundtrip_tests.rs"]
mod roundtrip_tests;

#[cfg(test)]
#[path = "issue_881_insert_binding_tests.rs"]
mod issue_881_insert_binding_tests;

#[cfg(test)]
#[path = "lib_misc_tests.rs"]
mod misc_tests;
