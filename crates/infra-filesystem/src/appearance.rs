//! Responsibility: names the colour scheme the interface paints in.
//!
//! ADR 0003 puts this in the SYSTEM `config.yaml`: how the screen looks is
//! about the machine it runs on, not about the rig a project carries.

use schemars::JsonSchema;
use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "lowercase")]
pub enum Appearance {
    /// Follows the operating system's light/dark setting.
    #[default]
    System,
    Light,
    Dark,
}
