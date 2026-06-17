//! Per-machine I/O binding registry types.
//!
//! An `IoBinding` captures how one physical audio interface maps to named
//! logical endpoints. Multiple bindings are stored in the system-level registry
//! (see ADR 0003 — system vs project config) so that projects can reference
//! endpoints by name without hardcoding device paths.

use domain::ids::DeviceId;
use serde::{Deserialize, Serialize};

use crate::chain::ChainInputMode;

/// A single named endpoint (input or output channel group) on a physical device.
///
/// Reuses `DeviceId` and `ChainInputMode` from the existing block types so
/// that the registry stays in sync with chain I/O configuration vocabulary.
/// `mode` follows the Mono/Stereo/DualMono conventions used by `InputEntry`.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct IoEndpoint {
    /// Human-readable label for this endpoint (e.g. "Guitar In 1").
    pub name: String,
    /// Stable identifier of the physical device that owns this endpoint.
    pub device_id: DeviceId,
    /// Channel layout: Mono, Stereo, or DualMono.
    #[serde(default)]
    pub mode: ChainInputMode,
    /// Zero-based channel indices on the device.
    pub channels: Vec<usize>,
}

/// A complete I/O binding: a named group of input + output endpoints on one
/// or more physical devices, identified by a stable `id`.
///
/// Stored in the per-machine system config registry so it survives project
/// portability (see ADR 0003).
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
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
