use crate::model::{AudioMode, Availability, BlockType, Classification, Plugin};
use anyhow::Result;
use heck::ToSnakeCase;
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::path::Path;

#[derive(Debug, Default, Deserialize, Serialize)]
pub struct OverridesFile {
    #[serde(default)]
    pub plugins: HashMap<String, PluginOverride>,
    #[serde(default)]
    pub bundles: HashMap<String, BundleOverride>,
}

#[derive(Debug, Default, Deserialize, Serialize)]
pub struct PluginOverride {
    pub block_type: Option<String>,
    pub skip: Option<bool>,
    pub skip_reason: Option<String>,
    pub display_name: Option<String>,
    pub model_id: Option<String>,
    pub brand: Option<String>,
}

#[derive(Debug, Default, Deserialize, Serialize)]
pub struct BundleOverride {
    pub brand: Option<String>,
    pub skip: Option<bool>,
    pub skip_reason: Option<String>,
}

#[derive(Debug, Default, Deserialize, Serialize)]
pub struct CrossPlatformMap {
    #[serde(default)]
    pub bundles: HashMap<String, CrossEntry>,
}

#[derive(Debug, Default, Deserialize, Serialize)]
pub struct CrossEntry {
    #[serde(default)]
    pub vst3_equivalent: Option<String>,
    #[serde(default)]
    pub clap_equivalent: Option<String>,
    #[serde(default)]
    pub notes: Option<String>,
}

pub fn load_overrides(path: &Path) -> Result<OverridesFile> {
    if !path.exists() {
        return Ok(OverridesFile::default());
    }
    let raw = std::fs::read_to_string(path)?;
    Ok(serde_yaml::from_str(&raw)?)
}

pub fn load_cross_map(path: &Path) -> Result<CrossPlatformMap> {
    if !path.exists() {
        return Ok(CrossPlatformMap::default());
    }
    let raw = std::fs::read_to_string(path)?;
    Ok(serde_yaml::from_str(&raw)?)
}

pub fn classify(
    plugin: &Plugin,
    overrides: &OverridesFile,
    cross_map: &CrossPlatformMap,
) -> Classification {
    let plugin_override = overrides.plugins.get(&plugin.uri);
    let bundle_override = overrides.bundles.get(&plugin.bundle_dir);

    if bundle_override.and_then(|b| b.skip).unwrap_or(false) {
        return Classification {
            block_type: None,
            audio_mode: None,
            availability: Availability::Skip,
            skip_reason: bundle_override
                .and_then(|b| b.skip_reason.clone())
                .or_else(|| Some("bundle override skip".to_string())),
            brand: derive_brand(plugin, bundle_override, plugin_override),
            model_id: derive_model_id(plugin, plugin_override),
            display_name: derive_display_name(plugin, plugin_override),
        };
    }
    if plugin_override.and_then(|p| p.skip).unwrap_or(false) {
        return Classification {
            block_type: None,
            audio_mode: None,
            availability: Availability::Skip,
            skip_reason: plugin_override
                .and_then(|p| p.skip_reason.clone())
                .or_else(|| Some("plugin override skip".to_string())),
            brand: derive_brand(plugin, bundle_override, plugin_override),
            model_id: derive_model_id(plugin, plugin_override),
            display_name: derive_display_name(plugin, plugin_override),
        };
    }

    let block_type_from_override = plugin_override
        .and_then(|p| p.block_type.as_deref())
        .and_then(parse_block_type);

    let block_type = block_type_from_override.or_else(|| auto_classify_block_type(plugin));

    let availability = if cross_map.bundles.contains_key(&plugin.bundle_dir) {
        Availability::Cross
    } else {
        Availability::LinuxOnly
    };

    let (audio_mode, skip_reason) = match block_type {
        Some(_) => match audio_mode_from_ports(plugin) {
            Some(m) => (Some(m), None),
            None => (
                None,
                Some(format!(
                    "no usable audio I/O (in={}, out={})",
                    plugin.audio_in_count(),
                    plugin.audio_out_count()
                )),
            ),
        },
        None => (
            None,
            Some(format!(
                "no block-type match (classes={:?})",
                plugin.plugin_classes
            )),
        ),
    };

    let final_block_type = if skip_reason.is_some() {
        None
    } else {
        block_type
    };
    let final_availability = if skip_reason.is_some() {
        Availability::Skip
    } else {
        availability
    };

    Classification {
        block_type: final_block_type,
        audio_mode,
        availability: final_availability,
        skip_reason,
        brand: derive_brand(plugin, bundle_override, plugin_override),
        model_id: derive_model_id(plugin, plugin_override),
        display_name: derive_display_name(plugin, plugin_override),
    }
}

fn parse_block_type(s: &str) -> Option<BlockType> {
    match s {
        "reverb" => Some(BlockType::Reverb),
        "delay" => Some(BlockType::Delay),
        "mod" | "modulation" => Some(BlockType::Mod),
        "filter" => Some(BlockType::Filter),
        "dyn" | "dynamics" => Some(BlockType::Dyn),
        "pitch" => Some(BlockType::Pitch),
        "gain" => Some(BlockType::Gain),
        "util" | "utility" => Some(BlockType::Util),
        "wah" => Some(BlockType::Wah),
        "preamp" => Some(BlockType::Preamp),
        "amp" => Some(BlockType::Amp),
        "cab" => Some(BlockType::Cab),
        "body" => Some(BlockType::Body),
        _ => None,
    }
}

fn auto_classify_block_type(plugin: &Plugin) -> Option<BlockType> {
    for class in &plugin.plugin_classes {
        match class.as_str() {
            "ReverbPlugin" => return Some(BlockType::Reverb),
            "DelayPlugin" => return Some(BlockType::Delay),
            "ModulatorPlugin" | "ChorusPlugin" | "FlangerPlugin" | "PhaserPlugin" => {
                return Some(BlockType::Mod)
            }
            "FilterPlugin" | "EQPlugin" | "ParaEQPlugin" | "MultiEQPlugin" | "LowpassPlugin"
            | "HighpassPlugin" | "BandpassPlugin" | "AllpassPlugin" | "CombPlugin" => {
                return Some(BlockType::Filter)
            }
            "CompressorPlugin" | "DynamicsPlugin" | "ExpanderPlugin" | "LimiterPlugin"
            | "GatePlugin" => return Some(BlockType::Dyn),
            "PitchPlugin" => return Some(BlockType::Pitch),
            "DistortionPlugin" | "WaveshaperPlugin" => return Some(BlockType::Gain),
            // not-effect / skip:
            "InstrumentPlugin" | "GeneratorPlugin" | "OscillatorPlugin" | "AnalyserPlugin"
            | "SpectralPlugin" | "MixerPlugin" | "ConverterPlugin" | "FunctionPlugin" => {
                return None
            }
            _ => {}
        }
    }
    // ambiguous classes — skip auto, require override
    for class in &plugin.plugin_classes {
        match class.as_str() {
            "SimulatorPlugin" | "SpatialPlugin" | "EnvelopePlugin" => return None,
            _ => {}
        }
    }
    // pure UtilityPlugin only? → block-util
    if plugin.plugin_classes.iter().any(|c| c == "UtilityPlugin") {
        return Some(BlockType::Util);
    }
    None
}

fn audio_mode_from_ports(plugin: &Plugin) -> Option<AudioMode> {
    let ai = plugin.audio_in_count();
    let ao = plugin.audio_out_count();
    match (ai, ao) {
        (1, 1) => Some(AudioMode::DualMono),
        (1, 2) => Some(AudioMode::MonoToStereo),
        (2, 2) => Some(AudioMode::TrueStereo),
        (0, _) | (_, 0) => None,
        _ => None,
    }
}

/// Heuristic prefix → brand table. The first matching entry wins.
/// Keep ordered: longest/most-specific prefixes first.
const BRAND_PREFIX_MAP: &[(&str, &str)] = &[
    ("Airwindows-", "airwindows"),
    ("ChowKick", "chowdsp"),
    ("ChowCentaur", "chowdsp"),
    ("CHOWTapeModel", "chowdsp"),
    ("ChowMatrix", "chowdsp"),
    ("ChowPhaser", "chowdsp"),
    ("Dragonfly", "dragonfly"),
    ("DISTRHO", "distrho"),
    ("Black_Pearl", "blackpearl"),
    ("AVL_Drumkits", "avlinux"),
    ("AirFont", "airfont"),
    ("Zam", "zam"),
    ("MVerb", "mverb"),
    ("MaPitchshift", "mapitch"),
    ("Pitchotto", "pitchotto"),
    ("FluidSynth", "fluidplug"),
    ("FluidPercussion", "fluidplug"),
    ("FluidPlug", "fluidplug"),
    ("Nekobi", "nekobi"),
    ("PingPongPan", "ndc"),
    ("Vex", "ndc"),
    ("mod-caps-", "caps"),
    ("mod-mda-", "mda"),
    ("mod-cv-", "modcv"),
    ("mod-arctican-", "arctican"),
    ("mod-bigmuff", "bigmuff"),
    ("mod-2voices", "modkxstudio"),
    ("mod-supercapo", "modkxstudio"),
    ("mod-mixer", "modkxstudio"),
    ("mod-", "kxstudio"),
    ("calf-", "calf"),
    ("calf.", "calf"),
    ("calf", "calf"),
    ("gx_", "guitarix"),
    ("gx-", "guitarix"),
    ("gx.", "guitarix"),
    ("Gx", "guitarix"),
    ("artyfx-", "artyfx"),
    ("artyfx.", "artyfx"),
    ("artyfx", "artyfx"),
    ("tap-", "tap"),
    ("swh-", "swh"),
    ("fomp.", "fomp"),
    ("fomp-", "fomp"),
    ("fomp", "fomp"),
    ("lsp-", "lsp"),
    ("lsp.", "lsp"),
    ("x42-", "x42"),
    ("ZeroConvo", "x42"),
    ("zeroconvo", "x42"),
    ("fat1", "x42"),
    ("tinygain", "x42"),
    ("tuna", "x42"),
    ("b_synth", "setbfree"),
    ("b_reverb", "setbfree"),
    ("b_overdrive", "setbfree"),
    ("b_whirl", "setbfree"),
    ("ojd_schrammel", "schrammel"),
    ("rkr", "rakarrack"),
    ("invada-", "invada"),
    ("invada", "invada"),
    ("carla-", "carla"),
    ("sc-", "shiroclassics"),
    ("triceratops", "triceratops"),
    ("amsynth", "amsynth"),
    ("avocado", "remaincalm"),
    ("floaty", "remaincalm"),
    ("modulay", "remaincalm"),
    ("granulator", "remaincalm"),
    ("freakclip", "freak"),
    ("BollieDelay", "bollie"),
    ("bollie", "bollie"),
    ("mverb", "mverb"),
    ("wolf-shaper", "wolf"),
    ("wolf-spectrum", "wolf"),
    ("midi-", "moony"),
    ("ewham", "ewham"),
    ("AudioToCV", "moony"),
    ("PitchToCV", "moony"),
    ("infamous_plugins-", "infamous"),
    ("vitalium", "vitalium"),
    ("nrkr", "rkr"),
    ("sooperlooper", "sooperlooper"),
    ("AmsLfo", "amsynth"),
    ("amp-vts", "ams"),
    ("master_me", "trummerschlunk"),
    ("rt-neural-generic", "rtneural"),
    ("paranoia", "ndc"),
    ("FluidGm", "fluidplug"),
    ("FluidStrings", "fluidplug"),
    ("FluidBass", "fluidplug"),
    ("airwin2rack", "airwindows"),
    ("OJD_", "schrammel"),
    ("ZeroFx", "ndc"),
    ("MaGigaverb", "ndc"),
    ("gain-stage", "elephantonist"),
    ("vocal-", "elephantonist"),
    ("crystal-", "elephantonist"),
    ("Mud.lv2", "ndc"),
];

fn derive_brand(
    plugin: &Plugin,
    bundle_override: Option<&BundleOverride>,
    plugin_override: Option<&PluginOverride>,
) -> String {
    if let Some(b) = plugin_override.and_then(|p| p.brand.as_deref()) {
        return b.to_string();
    }
    if let Some(b) = bundle_override.and_then(|b| b.brand.as_deref()) {
        return b.to_string();
    }
    if let Some(b) = plugin.mod_brand.as_deref() {
        return sanitize_brand(b);
    }
    let bundle_stem = plugin
        .bundle_dir
        .strip_suffix(".lv2")
        .unwrap_or(&plugin.bundle_dir);
    for (prefix, brand) in BRAND_PREFIX_MAP {
        if bundle_stem.starts_with(prefix) {
            return brand.to_string();
        }
    }
    let leading = bundle_stem
        .split(['-', '_', '.'])
        .next()
        .unwrap_or(bundle_stem);
    sanitize_brand(leading)
}

fn sanitize_brand(s: &str) -> String {
    s.to_lowercase()
        .chars()
        .map(|c| if c.is_ascii_alphanumeric() { c } else { '_' })
        .collect::<String>()
        .trim_matches('_')
        .to_string()
}

fn derive_display_name(plugin: &Plugin, plugin_override: Option<&PluginOverride>) -> String {
    if let Some(n) = plugin_override.and_then(|p| p.display_name.as_deref()) {
        return n.to_string();
    }
    if let Some(n) = plugin.doap_name.as_deref() {
        return n.to_string();
    }
    if let Some(n) = plugin.mod_label.as_deref() {
        return n.to_string();
    }
    plugin
        .uri
        .rsplit(['/', '#'])
        .next()
        .unwrap_or(&plugin.uri)
        .to_string()
}

pub fn derive_model_id(plugin: &Plugin, plugin_override: Option<&PluginOverride>) -> String {
    if let Some(id) = plugin_override.and_then(|p| p.model_id.as_deref()) {
        return id.to_string();
    }
    let bundle_stem = plugin
        .bundle_dir
        .strip_suffix(".lv2")
        .unwrap_or(&plugin.bundle_dir);
    let bundle_slug = sanitize_id_segment(bundle_stem);
    let plugin_slug = derive_plugin_slug_from_uri(&plugin.uri);
    if plugin_slug.is_empty() {
        format!("lv2_{}", bundle_slug)
    } else if bundle_slug.contains(&plugin_slug) {
        // bundle name already disambiguates; keep slug for traceability when not a strict prefix
        if bundle_slug.ends_with(&plugin_slug) {
            format!("lv2_{}", bundle_slug)
        } else {
            format!("lv2_{}_{}", bundle_slug, plugin_slug)
        }
    } else {
        format!("lv2_{}_{}", bundle_slug, plugin_slug)
    }
}

/// Generic suffixes that aren't unique enough on their own (mono, stereo, sum, etc.).
/// When the last URI segment is one of these, prepend the parent segment.
const URI_GENERIC_SUFFIXES: &[&str] = &[
    "mono", "stereo", "sum", "left", "right", "l", "r", "m", "s", "in", "out", "lr", "rl",
    "1", "2", "3", "4", "5",
];

fn derive_plugin_slug_from_uri(uri: &str) -> String {
    let segments: Vec<&str> = uri
        .rsplit(['/', '#', ':'])
        .filter(|s| !s.is_empty())
        .collect();
    if segments.is_empty() {
        return String::new();
    }
    let last = segments[0];
    let last_lower = last.to_lowercase();
    let is_generic = URI_GENERIC_SUFFIXES.contains(&last_lower.as_str());
    let combined = if is_generic && segments.len() >= 2 {
        format!("{}_{}", segments[1], last)
    } else {
        last.to_string()
    };
    sanitize_id_segment(&combined)
}

fn sanitize_id_segment(s: &str) -> String {
    s.to_snake_case()
        .chars()
        .filter(|c| c.is_ascii_alphanumeric() || *c == '_')
        .collect::<String>()
        .trim_matches('_')
        .to_string()
}
