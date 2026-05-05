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
mod tests {
    use super::*;

    fn parse(yaml: &str) -> PluginManifest {
        serde_yaml::from_str(yaml).expect("manifest should parse")
    }

    #[test]
    fn parses_nam_manifest() {
        let yaml = r#"
manifest_version: 1
id: my_preamp
display_name: My Preamp
type: preamp
backend: nam
parameters:
  - name: gain
    display_name: Gain
    values: [10, 20, 30]
captures:
  - values: { gain: 10 }
    file: captures/gain10.nam
  - values: { gain: 20 }
    file: captures/gain20.nam
  - values: { gain: 30 }
    file: captures/gain30.nam
"#;

        let m = parse(yaml);

        assert_eq!(m.manifest_version, 1);
        assert_eq!(m.id, "my_preamp");
        assert_eq!(m.block_type, BlockType::Preamp);
        match m.backend {
            Backend::Nam {
                parameters,
                captures,
            } => {
                assert_eq!(parameters.len(), 1);
                assert_eq!(parameters[0].name, "gain");
                assert!(matches!(
                    parameters[0].values[0],
                    ParameterValue::Number(value) if value == 10.0
                ));
                assert_eq!(captures.len(), 3);
                assert_eq!(captures[0].file, PathBuf::from("captures/gain10.nam"));
            }
            other => panic!("expected NAM backend, got {other:?}"),
        }
    }

    #[test]
    fn parses_ir_manifest_with_no_parameters() {
        let yaml = r#"
manifest_version: 1
id: my_cab
display_name: My Cab
type: cab
backend: ir
captures:
  - values: {}
    file: ir/v30_4x12.wav
"#;

        let m = parse(yaml);

        assert_eq!(m.block_type, BlockType::Cab);
        match m.backend {
            Backend::Ir {
                parameters,
                captures,
            } => {
                assert!(parameters.is_empty(), "IR with no params");
                assert_eq!(captures.len(), 1);
            }
            other => panic!("expected IR backend, got {other:?}"),
        }
    }

    #[test]
    fn parses_lv2_manifest_with_all_slots() {
        let yaml = r#"
manifest_version: 1
id: my_fuzz
display_name: My Fuzz
type: gain_pedal
backend: lv2
plugin_uri: http://example.com/plugins/my-fuzz
binaries:
  macos-universal: bundles/my-fuzz.lv2/macos-universal/my-fuzz.dylib
  windows-x86_64:  bundles/my-fuzz.lv2/windows-x86_64/my-fuzz.dll
  windows-aarch64: bundles/my-fuzz.lv2/windows-aarch64/my-fuzz.dll
  linux-x86_64:    bundles/my-fuzz.lv2/linux-x86_64/my-fuzz.so
  linux-aarch64:   bundles/my-fuzz.lv2/linux-aarch64/my-fuzz.so
"#;

        let m = parse(yaml);

        assert_eq!(m.block_type, BlockType::GainPedal);
        match m.backend {
            Backend::Lv2 {
                plugin_uri,
                binaries,
            } => {
                assert_eq!(plugin_uri, "http://example.com/plugins/my-fuzz");
                assert_eq!(binaries.len(), 5);
                assert!(binaries.contains_key(&Lv2Slot::MacosUniversal));
                assert!(binaries.contains_key(&Lv2Slot::LinuxAarch64));
            }
            other => panic!("expected LV2 backend, got {other:?}"),
        }
    }

    #[test]
    fn parses_lv2_manifest_with_partial_slots() {
        let yaml = r#"
manifest_version: 1
id: linux_only_plugin
display_name: Linux Only
type: util
backend: lv2
plugin_uri: urn:example:linux-only
binaries:
  linux-x86_64: bundles/linux-only.lv2/linux-x86_64/plugin.so
  linux-aarch64: bundles/linux-only.lv2/linux-aarch64/plugin.so
"#;

        let m = parse(yaml);

        match m.backend {
            Backend::Lv2 { binaries, .. } => {
                assert_eq!(binaries.len(), 2);
                assert!(!binaries.contains_key(&Lv2Slot::MacosUniversal));
                assert!(!binaries.contains_key(&Lv2Slot::WindowsX86_64));
            }
            _ => panic!("expected LV2"),
        }
    }

    #[test]
    fn rejects_unknown_backend() {
        let yaml = r#"
manifest_version: 1
id: bad
display_name: Bad
type: util
backend: vst3
"#;
        let result: Result<PluginManifest, _> = serde_yaml::from_str(yaml);
        assert!(result.is_err(), "unknown backend should be rejected");
    }

    #[test]
    fn rejects_unknown_block_type() {
        let yaml = r#"
manifest_version: 1
id: bad
display_name: Bad
type: synthesizer
backend: nam
parameters: []
captures: []
"#;
        let result: Result<PluginManifest, _> = serde_yaml::from_str(yaml);
        assert!(result.is_err(), "unknown block type should be rejected");
    }

    #[test]
    fn round_trip_nam_preserves_data() {
        let original = PluginManifest {
            manifest_version: 1,
            id: "round_trip".to_string(),
            display_name: "Round Trip".to_string(),
            author: Some("test".to_string()),
            description: None,
            inspired_by: None,
            brand: None,
            thumbnail: None,
            photo: None,
            screenshot: None,
            brand_logo: None,
            license: None,
            homepage: None,
            sources: None,
            block_type: BlockType::Preamp,
            backend: Backend::Nam {
                parameters: vec![GridParameter {
                    name: "gain".to_string(),
                    display_name: Some("Gain".to_string()),
                    values: vec![ParameterValue::Number(10.0), ParameterValue::Number(20.0)],
                }],
                captures: vec![GridCapture {
                    values: BTreeMap::from([("gain".to_string(), ParameterValue::Number(10.0))]),
                    file: PathBuf::from("captures/g10.nam"),
                }],
            },
        };

        let yaml = serde_yaml::to_string(&original).expect("serialize");
        let decoded: PluginManifest = serde_yaml::from_str(&yaml).expect("deserialize");
        assert_eq!(original, decoded);
    }

    #[test]
    fn parses_nam_manifest_with_enum_string_parameters() {
        let yaml = r#"
manifest_version: 1
id: ampeg_svt
display_name: SVT Classic
type: amp
backend: nam
parameters:
  - name: tone
    values: [standard, ultra_hi, ultra_lo]
  - name: mic
    values: [md421, sm57]
captures:
  - values: { tone: standard, mic: md421 }
    file: captures/svt_standard_md421.nam
  - values: { tone: standard, mic: sm57 }
    file: captures/svt_standard_sm57.nam
  - values: { tone: ultra_hi, mic: md421 }
    file: captures/svt_ultra_hi_md421.nam
  - values: { tone: ultra_hi, mic: sm57 }
    file: captures/svt_ultra_hi_sm57.nam
  - values: { tone: ultra_lo, mic: md421 }
    file: captures/svt_ultra_lo_md421.nam
  - values: { tone: ultra_lo, mic: sm57 }
    file: captures/svt_ultra_lo_sm57.nam
"#;

        let m = parse(yaml);

        match m.backend {
            Backend::Nam {
                parameters,
                captures,
            } => {
                assert_eq!(parameters.len(), 2);
                assert_eq!(parameters[0].name, "tone");
                assert!(matches!(
                    parameters[0].values[0],
                    ParameterValue::Text(ref s) if s == "standard"
                ));
                assert_eq!(captures.len(), 6);
            }
            other => panic!("expected NAM backend, got {other:?}"),
        }
    }
}
