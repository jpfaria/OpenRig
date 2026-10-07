//! Responsibility: describes the TONE3000 account settings a machine keeps for itself.
//!
//! The Secret Key belongs to the person at the machine, not to the rig, so it
//! lives in the system `config.yaml` (ADR 0003) and never in a project. Its
//! `Debug` output only says whether a key is set, so the key cannot leak into
//! logs, crash reports or MCP payloads that print the config (#879).

use serde::{Deserialize, Serialize};

#[derive(Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct Tone3000Config {
    /// TONE3000 API v1 Secret Key (`t3k_cs_…`), sent as a Bearer token.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub api_key: Option<String>,
}

impl Tone3000Config {
    /// True when nothing is set, so the section stays out of `config.yaml`.
    pub fn is_empty(&self) -> bool {
        self.api_key.is_none()
    }
}

impl std::fmt::Debug for Tone3000Config {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        let key = if self.api_key.is_some() {
            "configured"
        } else {
            "not configured"
        };
        f.debug_struct("Tone3000Config")
            .field("api_key", &key)
            .finish()
    }
}
