//! Plugin manifest schema (YAML).
//!
//! A plugin package is a folder containing `manifest.yaml` plus the assets it
//! references. The manifest declares one of three backends — NAM, IR, or LV2 —
//! and the loader uses that to know how to instantiate the plugin at runtime.
//!
//! # Wire format
//!
//! ```yaml
//! manifest_version: 1
//! id: my_preamp
//! display_name: My Preamp
//! type: preamp
//! backend: nam
//! parameters: [...]
//! captures: [...]
//! ```
//!
//! See issue #287 for the full schema and the rationale behind the `(OS, arch)`
//! slot matrix used by the LV2 backend.

use std::collections::BTreeMap;
use std::path::PathBuf;

use serde::{Deserialize, Serialize};

/// Top-level descriptor of a plugin package.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct PluginManifest {
    /// Schema version. Bumped when the manifest layout changes in a way that
    /// requires migration. Older loaders refuse manifests with a higher value.
    pub manifest_version: u32,

    /// Stable identifier used by projects, presets, and the registry to
    /// reference this plugin. Must be unique within the catalog.
    pub id: String,

    /// Human-readable name shown in the block drawer and editor.
    pub display_name: String,

    /// Optional author/contact string.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub author: Option<String>,

    /// Optional free-form description.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub description: Option<String>,

    /// Optional "inspired by" hint (e.g. real-world hardware reference).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub inspired_by: Option<String>,

    /// Manufacturer / brand of the device this plugin models.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub brand: Option<String>,

    /// Thumbnail image (PNG) shown in block drawers, relative to the
    /// package root.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub thumbnail: Option<PathBuf>,

    /// Hero photo of the device, relative to the package root.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub photo: Option<PathBuf>,

    /// In-app screenshot showing the plugin UI, relative to the package root.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub screenshot: Option<PathBuf>,

    /// Brand logo image, relative to the package root.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub brand_logo: Option<PathBuf>,

    /// SPDX-style license identifier or vendor-specific tag.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub license: Option<String>,

    /// Upstream project / vendor URL.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub homepage: Option<String>,

    /// URLs identifying where the captures bundled in this package were
    /// downloaded from (e.g. tone3000 capture pages). One entry per source.
    /// Empty/absent for plugins whose captures aren't tracked back to a
    /// public URL.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub sources: Option<Vec<String>>,

    /// Which block category this plugin belongs to.
    #[serde(rename = "type")]
    pub block_type: BlockType,

    /// Backend-specific payload. Serialized flat alongside the common fields,
    /// discriminated by the `backend` tag.
    #[serde(flatten)]
    pub backend: Backend,
}

/// Block category. Mirrors the `block-*` crates in the workspace.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum BlockType {
    GainPedal,
    Preamp,
    Amp,
    Cab,
    Body,
    Reverb,
    Delay,
    Mod,
    Filter,
    Dyn,
    Wah,
    Pitch,
    Util,
}

/// Backend-specific payload of a plugin.
///
/// Serde tags this with `backend: native|nam|ir|lv2|vst3` and flattens
/// the per-variant fields into the top-level manifest, producing the
/// canonical YAML shape documented at the module level.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(tag = "backend", rename_all = "snake_case")]
pub enum Backend {
    /// Native Rust DSP compiled into the binary. The `runtime_id` keys
    /// into the in-memory `native_runtimes` table that each `block-*`
    /// crate populates at startup, where the actual schema/validate/build
    /// fn pointers live. No assets ship on disk.
    Native { runtime_id: String },
    /// Neural Amp Modeler captures arranged on a parameter grid.
    Nam {
        parameters: Vec<GridParameter>,
        captures: Vec<GridCapture>,
    },
    /// Impulse response captures (e.g. cabinets, acoustic bodies).
    Ir {
        #[serde(default)]
        parameters: Vec<GridParameter>,
        captures: Vec<GridCapture>,
    },
    /// Native LV2 plugin bundle with per-platform binaries.
    ///
    /// Binaries live under `platform/<slot>/<filename>` inside the package.
    /// LV2 metadata (TTL files) ships alongside each binary in the same
    /// `platform/<slot>/` directory.
    Lv2 {
        plugin_uri: String,
        binaries: BTreeMap<Lv2Slot, PathBuf>,
    },
    /// Native VST3 plugin bundle. Cross-platform `.vst3` directory with
    /// `Contents/<arch>/<plugin>` inside, ships as a single bundle inside
    /// the package at `bundles/<name>.vst3/`. The host loads the first
    /// audio plugin class from the bundle and routes the user's parameter
    /// set to the VST3 numeric IDs declared in `parameters[].vst3_id`.
    Vst3 {
        bundle: PathBuf,
        parameters: Vec<Vst3Parameter>,
    },
}

/// One parameter of a VST3 plugin, with the bridge between OpenRig's
/// schema-friendly value range (`min`..`max`, optional discrete `step`)
/// and the VST3 host's normalized 0.0..1.0 value space.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Vst3Parameter {
    /// Stable identifier under which the user's `ParameterSet` keys this
    /// parameter (e.g. "drive", "saturation", "mix").
    pub name: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub display_name: Option<String>,
    /// VST3 numeric parameter ID, exposed by the plugin's
    /// `IEditController::getParameterCount()` enumeration.
    pub vst3_id: u32,
    pub min: f64,
    pub max: f64,
    pub default: f64,
    /// Optional step for discrete UI knobs (1.0 percent, 0.5 dB, etc).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub step: Option<f64>,
    /// Optional scale: VST3 normalized value = user_value / scale.
    /// Set to 100.0 for percent-unit parameters; None = pass-through.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub scale: Option<f64>,
    /// Free-form unit hint shown next to the value (percent, db, hz, ms).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub unit: Option<String>,
}

/// A single value on a capture-grid axis.
///
/// Real-world OpenRig models use both numeric grids (NAM gain stops at
/// `[10, 20, 30]`) and enum-style discrete labels (IR `voicing: "48k_m"`,
/// NAM `tone: "ultra_hi"`). YAML treats them naturally; this enum lets
/// the schema accept both without forcing the author to quote numbers
/// or invent fake numeric mappings for string labels.
///
/// Untagged so that `values: [10, 20]` and `values: [standard, ultra_hi]`
/// both round-trip as written in YAML.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(untagged)]
pub enum ParameterValue {
    Number(f64),
    Text(String),
}

impl PartialEq for ParameterValue {
    fn eq(&self, other: &Self) -> bool {
        match (self, other) {
            (Self::Number(a), Self::Number(b)) => a.to_bits() == b.to_bits(),
            (Self::Text(a), Self::Text(b)) => a == b,
            _ => false,
        }
    }
}
impl Eq for ParameterValue {}

impl std::hash::Hash for ParameterValue {
    fn hash<H: std::hash::Hasher>(&self, state: &mut H) {
        match self {
            Self::Number(value) => {
                state.write_u8(0);
                state.write_u64(value.to_bits());
            }
            Self::Text(value) => {
                state.write_u8(1);
                value.hash(state);
            }
        }
    }
}

/// One axis of a NAM/IR capture grid.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct GridParameter {
    pub name: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub display_name: Option<String>,
    pub values: Vec<ParameterValue>,
}

/// One cell of the NAM/IR capture grid.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct GridCapture {
    /// Map of parameter name → value identifying this cell on the grid.
    /// Empty for IR plugins that have no parametric variation.
    #[serde(default)]
    pub values: BTreeMap<String, ParameterValue>,
    /// Path to the asset (`.nam` or `.wav`) relative to the plugin folder.
    pub file: PathBuf,
}

/// Build target slot for an LV2 binary.
///
/// macOS uses a single Universal Binary 2 (fat Mach-O) covering both Intel
/// and Apple Silicon. Windows and Linux each ship one binary per arch. All
/// slots are optional — a plugin shipping only a subset of platforms is
/// marked unavailable on the rest by the loader.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord, Serialize, Deserialize)]
pub enum Lv2Slot {
    #[serde(rename = "macos-universal")]
    MacosUniversal,
    #[serde(rename = "windows-x86_64")]
    WindowsX86_64,
    #[serde(rename = "windows-aarch64")]
    WindowsAarch64,
    #[serde(rename = "linux-x86_64")]
    LinuxX86_64,
    #[serde(rename = "linux-aarch64")]
    LinuxAarch64,
}

#[cfg(test)]
#[path = "manifest_tests.rs"]
mod tests;
