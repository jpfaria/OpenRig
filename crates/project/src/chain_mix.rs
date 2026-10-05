//! Responsibility: describes a chain's own mixer faders as they travel in the project.
//! #1007 — the compact chain view mixes ONE chain: a fader per input and
//! output endpoint it plays through (on top of the global per-endpoint
//! mixer, never changing it) and a fader for its DI loop. They belong to the
//! chain, so they travel with `project.yaml` (ADR 0003). An endpoint is
//! named by its binding id + endpoint name, never by device, so the file
//! stays portable across machines.

use domain::mixer_strip::MixerDirection;
use serde::{Deserialize, Serialize};

/// The chain-local fader of one I/O endpoint.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, schemars::JsonSchema)]
pub struct ChainEndpointMix {
    pub direction: MixerDirection,
    pub binding_id: String,
    pub endpoint: String,
    /// Fader position in dB; 0 = unity.
    #[serde(default)]
    pub gain_db: f32,
    #[serde(default)]
    pub muted: bool,
}

/// Every chain-local fader of one chain. Default = everything at unity.
#[derive(Debug, Clone, PartialEq, Default, Serialize, Deserialize, schemars::JsonSchema)]
pub struct ChainMix {
    /// The DI loop's fader in dB; 0 = unity.
    #[serde(default, skip_serializing_if = "is_unity")]
    pub di_gain_db: f32,
    /// Only the endpoints somebody moved; a missing one is at unity.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub endpoints: Vec<ChainEndpointMix>,
}

fn is_unity(gain_db: &f32) -> bool {
    *gain_db == 0.0
}

impl ChainMix {
    /// Nothing moved: the chain serializes without a `mix` key.
    pub fn is_unity(&self) -> bool {
        self.di_gain_db == 0.0 && self.endpoints.is_empty()
    }

    /// The fader of one endpoint, if somebody moved it.
    pub fn endpoint(
        &self,
        direction: MixerDirection,
        binding_id: &str,
        endpoint: &str,
    ) -> Option<&ChainEndpointMix> {
        self.endpoints.iter().find(|e| {
            e.direction == direction && e.binding_id == binding_id && e.endpoint == endpoint
        })
    }

    /// The fader of one endpoint, created at unity when missing.
    pub fn endpoint_mut(
        &mut self,
        direction: MixerDirection,
        binding_id: &str,
        endpoint: &str,
    ) -> &mut ChainEndpointMix {
        let found = self.endpoints.iter().position(|e| {
            e.direction == direction && e.binding_id == binding_id && e.endpoint == endpoint
        });
        let index = found.unwrap_or_else(|| {
            self.endpoints.push(ChainEndpointMix {
                direction,
                binding_id: binding_id.to_string(),
                endpoint: endpoint.to_string(),
                gain_db: 0.0,
                muted: false,
            });
            self.endpoints.len() - 1
        });
        &mut self.endpoints[index]
    }

    /// Forget the endpoints back at unity and unmuted, so an untouched
    /// chain keeps writing the same file it always did.
    pub fn prune(&mut self) {
        self.endpoints.retain(|e| e.gain_db != 0.0 || e.muted);
    }
}

#[cfg(test)]
#[path = "chain_mix_tests.rs"]
mod tests;
