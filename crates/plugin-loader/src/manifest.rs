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

    /// Output gain do plugin como offset ADITIVO em **dB** (issue #491).
    ///
    /// - `0.0`   = unity (sem mudança no output do plugin)
    /// - `+6.0`  = output +6 dB (≈ ×2.0 em amplitude linear)
    /// - `-6.0`  = output −6 dB (≈ ×0.5)
    ///
    /// **Baseline de calibração**: o `nam_loudness_audit` mede cada
    /// plugin com sinal de teste fixo e escreve `output_gain_db` pra
    /// que o output do plugin fique num nível de referência calibrado
    /// isolado. Plugins NAM têm range natural de output muito variável
    /// (algumas capturas saem em -25 LUFS, outras em 0 LUFS); o offset
    /// em dB do manifest nivela esse baseline.
    ///
    /// **Combinação com `preset.volume`**: em runtime, o backend soma
    /// `output_gain_db` (baseline) ao nível do plugin e o engine aplica
    /// `preset.volume` (controle do usuário) por cima. Os dois entram
    /// em série.
    ///
    /// Ausente = `0.0 dB` (default unity). É o **mesmo nome e unidade**
    /// que o `nam_loudness_audit` escreve — contrato cross-repo single
    /// source of truth, sem serde alias.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub output_gain_db: Option<f32>,

    /// Manifest-level noise-gate defaults (issue #675).
    ///
    /// High-gain NAM captures amplify the input noise floor (~+32 dB) into
    /// audible idle hiss; the gate (already in the chain before the model)
    /// cuts it but defaults OFF (#612). This lets a capture ship the gate
    /// pre-regulated. Applies to every capture; a `GridCapture.noise_gate`
    /// overrides it per capture. Absent (the common case, and all IR
    /// plugins) → engine `DEFAULT_PLUGIN_PARAMS` (gate off).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub noise_gate: Option<ManifestNoiseGate>,

    /// NAM model architecture summary for the whole plugin (issue #650).
    ///
    /// Every NAM plugin is uniform — all its captures share one architecture
    /// — so a single per-plugin value is enough for the catalog to label and
    /// filter NAM/A1 vs NAM/A2 **without opening any `.nam`**. The `.nam`
    /// itself still carries the ground-truth architecture; this is a cached
    /// summary written by OpenRig-plugins.
    ///
    /// Absent for IR plugins and any pre-#650 (legacy) NAM manifest, both of
    /// which deserialize to `None`.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub architecture: Option<NamArchitecture>,

    /// Which block category this plugin belongs to.
    #[serde(rename = "type")]
    pub block_type: BlockType,

    /// Backend-specific payload. Serialized flat alongside the common fields,
    /// discriminated by the `backend` tag.
    #[serde(flatten)]
    pub backend: Backend,
}

impl PluginManifest {
    /// Map of VST3 numeric parameter id → author-declared tab group, for a
    /// `backend: vst3` manifest. Only parameters that declare a `group` are
    /// included; a non-VST3 manifest yields an empty map. The block editor
    /// overlays this onto the live parameters by `vst3_id` to render tabs
    /// (#780).
    pub fn vst3_group_map(&self) -> BTreeMap<u32, String> {
        match &self.backend {
            Backend::Vst3 { parameters, .. } => parameters
                .iter()
                .filter_map(|p| p.group.clone().map(|g| (p.vst3_id, g)))
                .collect(),
            _ => BTreeMap::new(),
        }
    }
}

/// Find the OpenRig package manifest that owns a scanned VST3 `bundle_path`
/// and return its `vst3_id → group` map. The catalog scans the raw `.vst3`
/// folder (e.g. `<plugins>/vst3/<id>/bundles/<name>.vst3`), so this walks up
/// a few levels looking for the sibling `manifest.yaml`. Returns an empty map
/// when the bundle ships no OpenRig manifest — the caller then groups the
/// parameters dynamically (#780).
pub fn vst3_group_map_for_bundle(bundle_path: &std::path::Path) -> BTreeMap<u32, String> {
    let mut dir = bundle_path.parent();
    for _ in 0..4 {
        let Some(d) = dir else { break };
        let candidate = d.join("manifest.yaml");
        if candidate.is_file() {
            if let Ok(text) = std::fs::read_to_string(&candidate) {
                if let Ok(manifest) = serde_yaml::from_str::<PluginManifest>(&text) {
                    return manifest.vst3_group_map();
                }
            }
        }
        dir = d.parent();
    }
    BTreeMap::new()
}

/// NAM model architecture family (issue #650).
///
/// - [`A1`](NamArchitecture::A1) — NAM "v1": WaveNet / LSTM / ConvNet
///   (any `.nam` whose architecture is *not* `SlimmableContainer`).
/// - [`A2`](NamArchitecture::A2) — NAM "v2": `SlimmableContainer`
///   (`.nam` version `0.7.0`).
///
/// Serialized UPPERCASE (`A1` / `A2`) to match the manifest wire format.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "UPPERCASE")]
pub enum NamArchitecture {
    A1,
    A2,
}

impl NamArchitecture {
    /// Short uppercase tag (`"A1"` / `"A2"`), matching the manifest wire
    /// format. Single source for the catalog badge and any mismatch warning.
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::A1 => "A1",
            Self::A2 => "A2",
        }
    }
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
    /// Native VST3 plugin hosted through its own editor (issue #776). Unlike
    /// the sonic categories above, this is not a DSP family — it marks a
    /// package whose `backend: vst3` bundle is surfaced through the same VST3
    /// block kind and discovery path as a system-installed VST3, so the
    /// manifest deserializes instead of being skipped.
    Vst3,
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
    /// Tab this parameter belongs to in the block editor. Plugins with
    /// hundreds of flat parameters (JUCE "Root Unit") declare groups so
    /// the editor renders one tab per group instead of one long knob wall
    /// (#780). `None` → the app groups the parameter dynamically.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub group: Option<String>,
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
    /// Order matters for serde untagged: try `Bool` before `Number`/`Text`,
    /// otherwise `true`/`false` would deserialize to `Text("true")` and the
    /// grid parameter would render as a string enum instead of a toggle.
    Bool(bool),
    Number(f64),
    Text(String),
}

impl PartialEq for ParameterValue {
    fn eq(&self, other: &Self) -> bool {
        match (self, other) {
            (Self::Bool(a), Self::Bool(b)) => a == b,
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
            Self::Bool(value) => {
                state.write_u8(0);
                state.write_u8(if *value { 1 } else { 0 });
            }
            Self::Number(value) => {
                state.write_u8(1);
                state.write_u64(value.to_bits());
            }
            Self::Text(value) => {
                state.write_u8(2);
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

/// Noise-gate defaults a NAM plugin can ship in its manifest (issue #675).
///
/// Both fields are optional so a manifest can set only what it needs
/// (e.g. just `threshold_db`); an absent field falls back to the next
/// layer (per-capture → manifest-level → engine `DEFAULT_PLUGIN_PARAMS`).
/// `threshold_db` uses the same input-referred dB convention the engine
/// already uses for `noise_gate.threshold_db`.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct ManifestNoiseGate {
    /// Whether the gate is engaged.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub enabled: Option<bool>,
    /// Gate threshold in input-referred dB.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub threshold_db: Option<f32>,
}

/// Resolve the effective noise-gate fields for a capture (issue #675): a
/// per-capture override wins per field over the manifest-level default.
/// Returns `(enabled, threshold_db)`, each `None` when neither layer sets
/// it — the caller then leaves the schema default in place. Used to SEED
/// the user-visible knobs at block creation (mirrors the `output_gain_db`
/// seeding), never applied as a hidden load-time default.
pub fn resolve_noise_gate(
    capture: Option<&ManifestNoiseGate>,
    manifest: Option<&ManifestNoiseGate>,
) -> (Option<bool>, Option<f32>) {
    let enabled = capture
        .and_then(|g| g.enabled)
        .or(manifest.and_then(|g| g.enabled));
    let threshold_db = capture
        .and_then(|g| g.threshold_db)
        .or(manifest.and_then(|g| g.threshold_db));
    (enabled, threshold_db)
}

/// One cell of the NAM/IR capture grid.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct GridCapture {
    /// Map of parameter name → value identifying this cell on the grid.
    /// Empty for IR plugins that have no parametric variation.
    #[serde(default)]
    pub values: BTreeMap<String, ParameterValue>,
    /// Path to the asset (`.nam` or `.wav`) relative to the plugin folder.
    pub file: PathBuf,
    /// Per-capture loudness audit baseline (issue #514). IR plugins
    /// ship one value per capture because each impulse has a different
    /// perceived level; falls back to
    /// [`PluginManifest::output_gain_db`] when absent. Same unit/sign
    /// convention as the top-level field (`+6.0` = +6 dB at output,
    /// `-6.0` = −6 dB).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub output_gain_db: Option<f32>,

    /// Per-capture noise-gate override (issue #675). Overrides
    /// [`PluginManifest::noise_gate`] for this capture; an absent field
    /// inherits the manifest-level value. Lets a single plugin ship the
    /// gate ON only for its high-gain captures and OFF for clean ones.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub noise_gate: Option<ManifestNoiseGate>,
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

#[cfg(test)]
#[path = "manifest_more_tests.rs"]
mod manifest_more;
