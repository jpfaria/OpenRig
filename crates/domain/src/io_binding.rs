//! Single source of truth for I/O binding types.
//!
//! `IoBinding` and `IoEndpoint` describe how physical audio devices map to
//! named logical endpoints. They live in `domain` so both `project` (which
//! embeds per-project references by endpoint name) and `infra-filesystem`
//! (which stores the per-machine registry in `config.yaml`) share one
//! definition with no duplication.

use schemars::JsonSchema;
use serde::{Deserialize, Serialize};

use crate::ids::DeviceId;

/// Channel layout for an I/O endpoint.
///
/// Mirrors the vocabulary used by `ChainInputMode` so that endpoint
/// declarations stay in sync with chain I/O configuration.
///
/// Serde wire format: `mono` / `stereo` / `dual_mono` (snake_case).
#[derive(
    Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, Default, JsonSchema,
)]
#[serde(rename_all = "snake_case")]
pub enum ChannelMode {
    /// Single-channel; upmixed to stereo for stereo outputs.
    #[default]
    Mono,
    /// Two-channel true stereo L/R pair.
    Stereo,
    /// Two independent mono pipelines (e.g. two guitars on separate inputs).
    DualMono,
}

/// A single named endpoint (input or output channel group) on a physical device.
///
/// Stored in the per-machine system config registry so that projects can
/// reference endpoints by name without hardcoding device paths (ADR 0003).
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
pub struct IoEndpoint {
    /// Human-readable label (e.g. `"Guitar In 1"`).
    pub name: String,
    /// Stable identifier of the physical device that owns this endpoint.
    pub device_id: DeviceId,
    /// Channel layout: Mono, Stereo, or DualMono.
    #[serde(default)]
    pub mode: ChannelMode,
    /// Zero-based channel indices on the device.
    pub channels: Vec<usize>,
}

/// A complete I/O binding: a named group of input + output endpoints on one
/// or more physical devices, identified by a stable `id`.
///
/// Stored in the per-machine system config registry so it survives project
/// portability (ADR 0003). Projects reference bindings by `id`, not by
/// device path, so `.openrig` files stay portable across machines.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
pub struct IoBinding {
    /// Stable registry key (e.g. `"main"`, `"monitor"`).
    pub id: String,
    /// Human-readable display name (e.g. `"Scarlett 2i2"`).
    pub name: String,
    /// Input endpoints exposed by this binding.
    pub inputs: Vec<IoEndpoint>,
    /// Output endpoints exposed by this binding.
    pub outputs: Vec<IoEndpoint>,
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::ids::DeviceId;

    // ── ChannelMode wire tokens ──────────────────────────────────────────

    #[test]
    fn channel_mode_mono_serializes_to_mono() {
        let s = serde_yaml::to_string(&ChannelMode::Mono).unwrap();
        assert_eq!(s.trim(), "mono");
    }

    #[test]
    fn channel_mode_stereo_serializes_to_stereo() {
        let s = serde_yaml::to_string(&ChannelMode::Stereo).unwrap();
        assert_eq!(s.trim(), "stereo");
    }

    #[test]
    fn channel_mode_dual_mono_serializes_to_dual_mono() {
        let s = serde_yaml::to_string(&ChannelMode::DualMono).unwrap();
        assert_eq!(s.trim(), "dual_mono");
    }

    #[test]
    fn channel_mode_mono_deserializes_from_mono() {
        let v: ChannelMode = serde_yaml::from_str("mono").unwrap();
        assert_eq!(v, ChannelMode::Mono);
    }

    #[test]
    fn channel_mode_stereo_deserializes_from_stereo() {
        let v: ChannelMode = serde_yaml::from_str("stereo").unwrap();
        assert_eq!(v, ChannelMode::Stereo);
    }

    #[test]
    fn channel_mode_dual_mono_deserializes_from_dual_mono() {
        let v: ChannelMode = serde_yaml::from_str("dual_mono").unwrap();
        assert_eq!(v, ChannelMode::DualMono);
    }

    // ── IoBinding round-trip ─────────────────────────────────────────────

    #[test]
    fn io_binding_round_trip_with_inputs_and_outputs() {
        let binding = IoBinding {
            id: "main".into(),
            name: "Scarlett 2i2".into(),
            inputs: vec![IoEndpoint {
                name: "Guitar In 1".into(),
                device_id: DeviceId("dev-001".into()),
                mode: ChannelMode::Mono,
                channels: vec![0],
            }],
            outputs: vec![IoEndpoint {
                name: "Monitor Out".into(),
                device_id: DeviceId("dev-001".into()),
                mode: ChannelMode::Stereo,
                channels: vec![0, 1],
            }],
        };
        let yaml = serde_yaml::to_string(&binding).unwrap();
        let restored: IoBinding = serde_yaml::from_str(&yaml).unwrap();
        assert_eq!(binding, restored);
    }

    #[test]
    fn io_binding_round_trip_preserves_dual_mono_endpoint() {
        let binding = IoBinding {
            id: "dual".into(),
            name: "Dual Guitar".into(),
            inputs: vec![IoEndpoint {
                name: "Guitar Pair".into(),
                device_id: DeviceId("dev-002".into()),
                mode: ChannelMode::DualMono,
                channels: vec![0, 1],
            }],
            outputs: vec![],
        };
        let yaml = serde_yaml::to_string(&binding).unwrap();
        let restored: IoBinding = serde_yaml::from_str(&yaml).unwrap();
        assert_eq!(binding, restored);
        assert_eq!(restored.inputs[0].mode, ChannelMode::DualMono);
    }
}
