//! Per-effect-type dispatch — the three big match arms that route a
//! block (`effect_type`, `model`) to the right `block-*` crate's schema /
//! validation / kind constructor, plus two private describe helpers.
//!
//! Lifted out of `block.rs` (Phase 7 of issue #194). One responsibility:
//! cross-crate dispatch on `effect_type`. The acceptance criterion's
//! second half ("dispatch via trait") is a follow-up that would replace
//! the match chains with a registry-backed trait — out of scope for the
//! file split.

use block_amp::{amp_model_schema, validate_amp_params};
use block_body::{body_model_schema, validate_body_params};
use block_cab::{cab_model_schema, validate_cab_params};
use block_delay::delay_model_schema;
use block_dyn::dynamics_model_schema;
use block_filter::filter_model_schema;
use block_full_rig::{full_rig_model_schema, validate_full_rig_params};
use block_gain::{gain_model_schema, validate_gain_params};
use block_ir::{ir_model_schema, validate_ir_params};
use block_mod::modulation_model_schema;
use block_nam::nam_model_schema;
use block_pitch::{pitch_model_schema, validate_pitch_params};
use block_preamp::{preamp_model_schema, validate_preamp_params};
use block_reverb::reverb_model_schema;
use block_util::utility_model_schema;
use block_wah::{validate_wah_params, wah_model_schema};
use domain::ids::BlockId;
use domain::value_objects::ParameterValue;

use crate::param::{BlockParameterDescriptor, ModelParameterSchema, ParameterSet};

use super::manifest_labels::sanitize_label;
use super::types::{AudioBlockKind, BlockAudioDescriptor, CoreBlock, NamBlock};

pub fn normalize_block_params(
    effect_type: &str,
    model: &str,
    params: ParameterSet,
) -> Result<ParameterSet, String> {
    let schema = schema_for_block_model(effect_type, model)?;
    let normalized = params.normalized_against(&schema)?;
    use block_core::*;
    let result: Result<(), anyhow::Error> = match effect_type {
        EFFECT_TYPE_PREAMP => validate_preamp_params(model, &normalized),
        EFFECT_TYPE_AMP => validate_amp_params(model, &normalized),
        EFFECT_TYPE_FULL_RIG => validate_full_rig_params(model, &normalized),
        EFFECT_TYPE_CAB => validate_cab_params(model, &normalized),
        EFFECT_TYPE_BODY => validate_body_params(model, &normalized),
        EFFECT_TYPE_IR => validate_ir_params(model, &normalized),
        EFFECT_TYPE_GAIN => validate_gain_params(model, &normalized),
        EFFECT_TYPE_WAH => validate_wah_params(model, &normalized),
        EFFECT_TYPE_PITCH => validate_pitch_params(model, &normalized),
        _ => Ok(()),
    };
    if let Err(error) = result {
        // Disk packages don't live in the legacy block-* registry that
        // validate_*_params consults, so an Err here is expected for
        // disk-backed models — accept them and let the package builder
        // surface any real param mismatch at instantiation time. Issue #287.
        if plugin_loader::registry::find(model).is_none() {
            return Err(error.to_string());
        }
    }
    Ok(normalized)
}

pub fn schema_for_block_model(
    effect_type: &str,
    model: &str,
) -> Result<ModelParameterSchema, String> {
    schema_for_block_model_legacy(effect_type, model)
        .or_else(|legacy_err| schema_from_disk_package(effect_type, model, legacy_err))
}

/// Fallback: if the legacy block-* registry doesn't recognise the model,
/// try `plugin_loader::registry`. Synthesizes a parameter schema from
/// the manifest data so the GUI can render knobs for disk-backed
/// plugins. Issue: #287.
fn schema_from_disk_package(
    effect_type: &str,
    model: &str,
    legacy_err: String,
) -> Result<ModelParameterSchema, String> {
    let package = plugin_loader::registry::find(model).ok_or(legacy_err)?;
    let parameters = synthesize_parameters_from_manifest(package);
    Ok(ModelParameterSchema {
        effect_type: effect_type.to_string(),
        model: package.manifest.id.clone(),
        display_name: package.manifest.display_name.clone(),
        // Streams are ALWAYS stereo internally (CLAUDE.md invariant #5).
        // Mono-native plugins (NAM, IR, LV2 1in/1out) run as DualMono:
        // one instance per channel. Forcing MonoOnly here would make
        // the engine downmix to mono, violating the stereo invariant.
        audio_mode: block_core::ModelAudioMode::DualMono,
        parameters,
    })
}

/// Build a `Vec<ParameterSpec>` from a `LoadedPackage` manifest:
/// - NAM/IR: each grid axis becomes a float (numeric values) or enum
///   (text values) parameter spanning the declared min..max.
/// - LV2: scan the bundle's TTL files and emit a float param per
///   `ControlIn` port using its TTL min/max/default.
/// - VST3: each declared `Vst3Parameter` becomes a 0..1 float param.
/// - Native: nothing — natives go through the legacy schema path so
///   this branch shouldn't fire in practice; return empty to avoid a
///   panic if it ever does.
pub(crate) fn synthesize_parameters_from_manifest(
    package: &plugin_loader::LoadedPackage,
) -> Vec<block_core::param::ParameterSpec> {
    use plugin_loader::manifest::Backend;
    match &package.manifest.backend {
        Backend::Nam {
            parameters,
            captures,
        } => {
            // Pre-#287 (when NAM amps lived in `block-preamp/src/nam_*.rs`),
            // every NAM model exposed two layers of knobs: the per-capture
            // grid (e.g. `mode`, `character` for nam_boss_ds_2) AND the 8
            // universal NAM plugin knobs (input/output level, noise gate,
            // EQ on/off + bass/mid/treble) added by `nam::plugin_parameter_specs()`.
            // The migration to disk packages dropped the second layer, so
            // every NAM in the GUI lost its standard knobs (~96 packages —
            // 21 with empty grids ended up with zero knobs at all). Merge
            // the standard set back in. Issue #401.
            //
            // `effective_grid_axes` first drops dead capture-selector axes
            // (single-value or over-declared dropdowns) — issue #649.
            let axes = plugin_loader::grid_axes::effective_grid_axes(parameters, captures);
            let mut specs: Vec<block_core::param::ParameterSpec> =
                axes.iter().map(grid_parameter_to_spec).collect();
            // Issue #496 reverses #402's "drop output_db". With the
            // audit-side `output_gain_db` cleared in the manifests,
            // there was no automatic compensation AND no user-facing
            // knob — every NAM played at the raw (quiet) capture
            // output. Re-expose the host's Output knob so the user
            // can add makeup gain; the manifest `output_gain_db` is
            // still summed on top when present.
            specs.extend(nam::processor::plugin_parameter_specs());
            // Issue #657: NAM/A2 (SlimmableContainer) models expose a
            // runtime size lever (SetSlimmableSize). A1 models are not
            // slimmable, so the knob is appended only for A2 — driven by
            // the manifest's declared architecture (issue #650).
            if package.manifest.architecture == Some(plugin_loader::manifest::NamArchitecture::A2) {
                specs.push(nam::processor::slim_parameter_spec());
            }
            specs
        }
        Backend::Ir {
            parameters,
            captures,
        } => {
            // Same dead-axis filter as NAM (issue #649).
            let axes = plugin_loader::grid_axes::effective_grid_axes(parameters, captures);
            let mut specs: Vec<block_core::param::ParameterSpec> =
                axes.iter().map(grid_parameter_to_spec).collect();
            // Issue #733: a `type: reverb` IR blends dry/wet rather than
            // playing 100% wet at a calibrated level, so it exposes the
            // reverb controls (mix / pre-delay / wet level) in place of the
            // cab-style absolute Output knob.
            if package.manifest.block_type == plugin_loader::manifest::BlockType::Reverb {
                specs.extend(block_reverb::ir_reverb_parameter_specs());
                return specs;
            }
            // Issue #655: user-adjustable Output Level knob (mirrors NAM).
            // The default mirrors the engine baseline — the first capture's
            // audit (manifest-level fallback, 0 dB if neither) — so the knob
            // shows the real applied offset and a fresh block born at the
            // first capture stays unchanged (volume invariant #10). The
            // audio path resolves the offset per-capture from the raw saved
            // params (see `ir::from_package::resolve_output_db`); this
            // default only drives the UI and the new-block seed.
            let default_db = captures
                .first()
                .and_then(|c| c.output_gain_db)
                .or(package.manifest.output_gain_db)
                .unwrap_or(0.0);
            specs.push(block_core::param::float_parameter(
                "output_db",
                "Output",
                None,
                Some(default_db),
                -24.0,
                24.0,
                0.1,
                block_core::param::ParameterUnit::Decibels,
            ));
            specs
        }
        Backend::Lv2 {
            plugin_uri,
            binaries,
        } => synthesize_lv2_parameters(package, plugin_uri, binaries),
        Backend::Vst3 { parameters, .. } => parameters
            .iter()
            .map(|param| {
                block_core::param::float_parameter(
                    &param.name,
                    param.display_name.as_deref().unwrap_or(&param.name),
                    None,
                    Some(param.default as f32),
                    param.min as f32,
                    param.max as f32,
                    param.step.unwrap_or(0.01) as f32,
                    block_core::param::ParameterUnit::None,
                )
            })
            .collect(),
        Backend::Native { .. } => Vec::new(),
    }
}

fn grid_parameter_to_spec(
    parameter: &plugin_loader::manifest::GridParameter,
) -> block_core::param::ParameterSpec {
    use plugin_loader::manifest::ParameterValue;
    // Sanitise the axis name so emojis baked into third-party manifests
    // (issue #424 — Bogner Ecstasy) don't tofu in the BlockEditorPanel
    // header; raw `parameter.name` stays untouched as the lookup key.
    let raw_label = parameter.display_name.as_deref().unwrap_or(&parameter.name);
    let label = sanitize_label(raw_label);
    let all_numeric = parameter
        .values
        .iter()
        .all(|v| matches!(v, ParameterValue::Number(_)));
    if all_numeric && !parameter.values.is_empty() {
        let numbers: Vec<f64> = parameter
            .values
            .iter()
            .filter_map(|v| match v {
                ParameterValue::Number(n) => Some(*n),
                _ => None,
            })
            .collect();
        let min = numbers.iter().cloned().fold(f64::INFINITY, f64::min);
        let max = numbers.iter().cloned().fold(f64::NEG_INFINITY, f64::max);
        let default = numbers.first().copied().unwrap_or(min);
        let step = if numbers.len() > 1 {
            (numbers[1] - numbers[0]).abs() as f32
        } else {
            1.0_f32
        };
        block_core::param::float_parameter(
            &parameter.name,
            &label,
            None,
            Some(default as f32),
            min as f32,
            max as f32,
            step.max(0.01),
            block_core::param::ParameterUnit::None,
        )
    } else if parameter
        .values
        .iter()
        .all(|v| matches!(v, ParameterValue::Bool(_)))
        && !parameter.values.is_empty()
    {
        // Pure bool grid → render as a toggle. The default mirrors the
        // first listed value so manifests can pick the natural off-state
        // (`[false, true]` -> default off).
        let default = parameter.values.iter().find_map(|v| match v {
            ParameterValue::Bool(b) => Some(*b),
            _ => None,
        });
        block_core::param::bool_parameter(&parameter.name, &label, None, default)
    } else {
        // (raw_value, sanitised_label) pairs — the value is the lookup
        // key into `captures[].values` and the user's persisted
        // `ParameterSet`, so it must round-trip byte-for-byte. Only the
        // displayed label is cleaned of emoji (issue #424).
        let options: Vec<(String, String)> = parameter
            .values
            .iter()
            .map(|value| {
                let raw = match value {
                    ParameterValue::Text(t) => t.clone(),
                    ParameterValue::Number(n) => n.to_string(),
                    ParameterValue::Bool(b) => b.to_string(),
                };
                let display = sanitize_label(&raw);
                (raw, display)
            })
            .collect();
        let option_refs: Vec<(&str, &str)> = options
            .iter()
            .map(|(value, display)| (value.as_str(), display.as_str()))
            .collect();
        let default = options.first().map(|(k, _)| k.as_str());
        block_core::param::enum_parameter(&parameter.name, &label, None, default, &option_refs)
    }
}

fn synthesize_lv2_parameters(
    package: &plugin_loader::LoadedPackage,
    plugin_uri: &str,
    binaries: &std::collections::BTreeMap<plugin_loader::manifest::Lv2Slot, std::path::PathBuf>,
) -> Vec<block_core::param::ParameterSpec> {
    use plugin_loader::dispatch::Lv2PortRole;
    // Prefer the deduplicated `<package>/data/` TTL bundle; fall back
    // to the legacy per-platform layout where TTLs lived next to the
    // binary. Either layout works.
    let data_dir = package.root.join("data");
    let bundle_dir: std::path::PathBuf = if data_dir.is_dir() {
        data_dir
    } else if let Some((_, rel_binary)) = binaries.iter().next() {
        let bin_path = package.root.join(rel_binary);
        match bin_path.parent() {
            Some(parent) => parent.to_path_buf(),
            None => return Vec::new(),
        }
    } else {
        return Vec::new();
    };
    let Ok(ports) = plugin_loader::dispatch::scan_lv2_ports(&bundle_dir, plugin_uri) else {
        return Vec::new();
    };
    ports
        .into_iter()
        .filter(|port| port.role == Lv2PortRole::ControlIn)
        .map(synthesize_one_lv2_param)
        .collect()
}

/// Translate one LV2 ControlIn port into the corresponding
/// `ParameterSpec`. Routing — checked in this order so that an
/// `enumeration + integer` port (a common pattern) lands as an enum:
///
/// 1. `lv2:toggled` → bool checkbox.
/// 2. `lv2:enumeration` with at least one `lv2:scalePoint` → enum dropdown.
/// 3. `lv2:integer` (no scalePoint) → integer-stepped float.
/// 4. otherwise → continuous float (legacy behaviour).
fn synthesize_one_lv2_param(
    port: plugin_loader::dispatch::Lv2Port,
) -> block_core::param::ParameterSpec {
    let label = port.name.clone().unwrap_or_else(|| port.symbol.clone());

    if port.is_toggle {
        let default = port.default_value.map(|value| value >= 0.5).or(Some(false));
        return block_core::param::bool_parameter(&port.symbol, &label, None, default);
    }

    if port.is_enumeration && !port.scale_points.is_empty() {
        // Enum values keep the original numeric ordering. Stored values
        // are the numeric `rdf:value` (stringified) so the runtime can
        // round-trip them back to the LV2 control port.
        let options: Vec<(String, String)> = port
            .scale_points
            .iter()
            .map(|sp| (sp.value.to_string(), sp.label.clone()))
            .collect();
        let options_refs: Vec<(&str, &str)> = options
            .iter()
            .map(|(value, label)| (value.as_str(), label.as_str()))
            .collect();
        let default = port
            .default_value
            .and_then(|value| {
                port.scale_points
                    .iter()
                    .find(|sp| (sp.value - value).abs() < f32::EPSILON)
            })
            .map(|sp| sp.value.to_string());
        return block_core::param::enum_parameter(
            &port.symbol,
            &label,
            None,
            default.as_deref(),
            &options_refs,
        );
    }

    let min = port.minimum.unwrap_or(0.0);
    let max = port.maximum.unwrap_or(1.0).max(min + 0.001);
    let default = port.default_value.unwrap_or((min + max) / 2.0);

    let step = if port.is_integer {
        // pprop:rangeSteps tells us exactly how many discrete positions
        // the host should expose; fall back to step=1 for plain integer
        // ports without explicit step count.
        port.range_steps
            .filter(|n| *n > 0)
            .map(|n| (max - min) / n as f32)
            .unwrap_or(1.0)
    } else {
        // Continuous control. step = 0 = "no snap-to-grid".
        0.0
    };

    block_core::param::float_parameter(
        &port.symbol,
        &label,
        None,
        Some(default),
        min,
        max,
        step,
        block_core::param::ParameterUnit::None,
    )
}

fn schema_for_block_model_legacy(
    effect_type: &str,
    model: &str,
) -> Result<ModelParameterSchema, String> {
    use block_core::*;
    match effect_type {
        EFFECT_TYPE_PREAMP => preamp_model_schema(model).map_err(|error| error.to_string()),
        EFFECT_TYPE_AMP => amp_model_schema(model).map_err(|error| error.to_string()),
        EFFECT_TYPE_FULL_RIG => full_rig_model_schema(model).map_err(|error| error.to_string()),
        EFFECT_TYPE_CAB => cab_model_schema(model).map_err(|error| error.to_string()),
        EFFECT_TYPE_BODY => body_model_schema(model).map_err(|error| error.to_string()),
        EFFECT_TYPE_IR => ir_model_schema(model).map_err(|error| error.to_string()),
        EFFECT_TYPE_GAIN => gain_model_schema(model).map_err(|error| error.to_string()),
        EFFECT_TYPE_NAM => nam_model_schema(model).map_err(|error| error.to_string()),
        EFFECT_TYPE_DELAY => delay_model_schema(model).map_err(|error| error.to_string()),
        EFFECT_TYPE_REVERB => reverb_model_schema(model).map_err(|error| error.to_string()),
        EFFECT_TYPE_UTILITY => utility_model_schema(model).map_err(|error| error.to_string()),
        EFFECT_TYPE_DYNAMICS => dynamics_model_schema(model).map_err(|error| error.to_string()),
        EFFECT_TYPE_FILTER => filter_model_schema(model).map_err(|error| error.to_string()),
        EFFECT_TYPE_WAH => wah_model_schema(model).map_err(|error| error.to_string()),
        EFFECT_TYPE_PITCH => pitch_model_schema(model).map_err(|error| error.to_string()),
        EFFECT_TYPE_MODULATION => modulation_model_schema(model).map_err(|error| error.to_string()),
        x if x == block_core::EFFECT_TYPE_VST3 => {
            let entry = vst3_host::find_vst3_plugin(model)
                .ok_or_else(|| format!("VST3 plugin '{}' not found in catalog", model))?;
            // Build a float parameter for each discovered VST3 parameter (normalized 0–100%).
            let parameters = entry
                .info
                .params
                .iter()
                .map(|p| {
                    let path = format!("p{}", p.id);
                    let label = if p.title.is_empty() {
                        p.short_title.clone()
                    } else {
                        p.title.clone()
                    };
                    let default_pct = (p.default_normalized * 100.0) as f32;
                    block_core::param::float_parameter(
                        &path,
                        &label,
                        None,
                        Some(default_pct),
                        0.0,
                        100.0,
                        1.0,
                        block_core::param::ParameterUnit::Percent,
                    )
                })
                .collect();
            Ok(ModelParameterSchema {
                effect_type: block_core::EFFECT_TYPE_VST3.to_string(),
                model: model.to_string(),
                display_name: entry.display_name.to_string(),
                audio_mode: ModelAudioMode::MonoToStereo,
                parameters,
            })
        }
        other => Err(format!("unsupported block type '{}'", other)),
    }
}

pub fn build_audio_block_kind(
    effect_type: &str,
    model: &str,
    params: ParameterSet,
) -> Result<AudioBlockKind, String> {
    let model = model.to_string();
    use block_core::*;
    let kind = match effect_type {
        EFFECT_TYPE_PREAMP
        | EFFECT_TYPE_AMP
        | EFFECT_TYPE_FULL_RIG
        | EFFECT_TYPE_CAB
        | EFFECT_TYPE_BODY
        | EFFECT_TYPE_IR
        | EFFECT_TYPE_GAIN
        | EFFECT_TYPE_DYNAMICS
        | EFFECT_TYPE_FILTER
        | EFFECT_TYPE_WAH
        | EFFECT_TYPE_PITCH
        | EFFECT_TYPE_MODULATION
        | EFFECT_TYPE_DELAY
        | EFFECT_TYPE_REVERB
        | EFFECT_TYPE_UTILITY => AudioBlockKind::Core(CoreBlock {
            effect_type: effect_type.to_string(),
            model,
            params,
        }),
        EFFECT_TYPE_NAM => AudioBlockKind::Nam(NamBlock { model, params }),
        x if x == EFFECT_TYPE_VST3 => AudioBlockKind::Core(CoreBlock {
            effect_type: EFFECT_TYPE_VST3.to_string(),
            model,
            params,
        }),
        other => return Err(format!("unsupported block type '{}'", other)),
    };
    Ok(kind)
}

pub(super) fn describe_block_params(
    block_id: &BlockId,
    effect_type: &str,
    model: &str,
    params: &ParameterSet,
) -> Result<Vec<BlockParameterDescriptor>, String> {
    let schema = schema_for_block_model(effect_type, model)?;
    let normalized = params.normalized_against(&schema)?;
    let ctx = block_core::param::MaterializeContext {
        block_id,
        effect_type,
        model,
        audio_mode: schema.audio_mode,
    };
    Ok(schema
        .parameters
        .iter()
        .map(|spec| {
            let current_value = normalized
                .get(&spec.path)
                .cloned()
                .or_else(|| spec.default_value.clone())
                .unwrap_or(ParameterValue::Null);
            spec.materialize(&ctx, current_value)
        })
        .collect())
}

pub(super) fn describe_block_audio(
    block_id: &BlockId,
    effect_type: &str,
    model: &str,
) -> Result<BlockAudioDescriptor, String> {
    let schema = schema_for_block_model(effect_type, model)?;
    Ok(BlockAudioDescriptor {
        block_id: block_id.clone(),
        effect_type: effect_type.to_string(),
        model: schema.model,
        display_name: schema.display_name,
        audio_mode: schema.audio_mode,
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use plugin_loader::manifest::{
        Backend, BlockType, GridCapture, GridParameter, ParameterValue, PluginManifest,
    };
    use plugin_loader::LoadedPackage;
    use std::path::PathBuf;

    fn nam_amp_package(
        id: &str,
        display_name: &str,
        axes: Vec<GridParameter>,
        captures: Vec<GridCapture>,
    ) -> LoadedPackage {
        LoadedPackage {
            root: PathBuf::from("/fake"),
            manifest: PluginManifest {
                manifest_version: 1,
                id: id.into(),
                display_name: display_name.into(),
                author: None,
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
                output_gain_db: None,
                noise_gate: None,
                architecture: None,
                block_type: BlockType::Amp,
                backend: Backend::Nam {
                    parameters: axes,
                    captures,
                },
            },
        }
    }

    fn nam_package_with_axes() -> LoadedPackage {
        nam_amp_package(
            "nam_test_amp",
            "Test NAM Amp",
            vec![GridParameter {
                name: "channel".into(),
                display_name: None,
                values: vec![
                    ParameterValue::Text("a".into()),
                    ParameterValue::Text("b".into()),
                ],
            }],
            vec![],
        )
    }

    #[test]
    fn nam_synthesized_schema_exposes_output_db_knob() {
        // Issue #496 reversed #402: when audit-side `output_gain_db`
        // values are zeroed (or absent), the user has no way to add
        // makeup gain on a quiet capture — the chain plays at the raw
        // model output, which is typically far below realistic amp
        // level. Exposing the Output knob gives the user manual control;
        // when a hot `output_gain_db` IS present in the manifest, it is
        // still applied automatically (the two coexist additively).
        let pkg = nam_package_with_axes();
        let specs = synthesize_parameters_from_manifest(&pkg);
        assert!(
            specs.iter().any(|s| s.path == "output_db"),
            "NAM schema must include `output_db` so the user can add \
             makeup gain when the manifest is zero; got params: {:?}",
            specs.iter().map(|s| &s.path).collect::<Vec<_>>()
        );
        assert!(
            specs.iter().any(|s| s.path == "input_db"),
            "NAM schema must include `input_db` (always was)"
        );
    }

    #[test]
    fn nam_a2_synthesized_schema_exposes_slim_knob() {
        // Issue #657: A2 (SlimmableContainer) models expose a runtime
        // `slim` size knob wired to SetSlimmableSize.
        use plugin_loader::manifest::NamArchitecture;
        let mut pkg = nam_package_with_axes();
        pkg.manifest.architecture = Some(NamArchitecture::A2);
        let specs = synthesize_parameters_from_manifest(&pkg);
        assert!(
            specs.iter().any(|s| s.path == "slim"),
            "NAM/A2 schema must expose the `slim` knob; got: {:?}",
            specs.iter().map(|s| &s.path).collect::<Vec<_>>()
        );
    }

    #[test]
    fn nam_a1_and_legacy_synthesized_schema_have_no_slim_knob() {
        // A1 models are not slimmable, and pre-#650 manifests have no
        // architecture at all — neither exposes the slim knob (issue #657).
        use plugin_loader::manifest::NamArchitecture;
        let mut a1 = nam_package_with_axes();
        a1.manifest.architecture = Some(NamArchitecture::A1);
        assert!(
            !synthesize_parameters_from_manifest(&a1)
                .iter()
                .any(|s| s.path == "slim"),
            "A1 NAM must NOT expose the slim knob (not slimmable)"
        );
        let legacy = nam_package_with_axes(); // architecture: None
        assert!(
            !synthesize_parameters_from_manifest(&legacy)
                .iter()
                .any(|s| s.path == "slim"),
            "legacy NAM (no architecture) must NOT expose the slim knob"
        );
    }

    fn ir_package_with_capture_audit(first_audit_db: Option<f32>) -> LoadedPackage {
        LoadedPackage {
            root: PathBuf::from("/fake"),
            manifest: PluginManifest {
                manifest_version: 1,
                id: "ir_test_body".into(),
                display_name: "Test IR".into(),
                author: None,
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
                output_gain_db: None,
                noise_gate: None,
                architecture: None,
                block_type: BlockType::Cab,
                backend: Backend::Ir {
                    parameters: vec![GridParameter {
                        name: "position".into(),
                        display_name: None,
                        values: vec![
                            ParameterValue::Text("a".into()),
                            ParameterValue::Text("b".into()),
                        ],
                    }],
                    captures: vec![
                        GridCapture {
                            values: [("position".to_string(), ParameterValue::Text("a".into()))]
                                .into_iter()
                                .collect(),
                            file: "a.wav".into(),
                            output_gain_db: first_audit_db,
                            noise_gate: None,
                        },
                        GridCapture {
                            values: [("position".to_string(), ParameterValue::Text("b".into()))]
                                .into_iter()
                                .collect(),
                            file: "b.wav".into(),
                            output_gain_db: Some(-10.0),
                            noise_gate: None,
                        },
                    ],
                },
            },
        }
    }

    #[test]
    fn ir_synthesized_schema_exposes_output_db_knob_in_decibels() {
        // Issue #655: IR blocks need a user-adjustable Output Level knob
        // (mirroring NAM) so resonant body IRs whose audit baseline cut
        // them far down can be brought back up. It must be a dB control.
        let pkg = ir_package_with_capture_audit(Some(-22.9));
        let specs = synthesize_parameters_from_manifest(&pkg);
        let output_db = specs
            .iter()
            .find(|s| s.path == "output_db")
            .expect("IR schema must include `output_db` so the user can adjust output level");
        assert_eq!(
            output_db.unit,
            block_core::param::ParameterUnit::Decibels,
            "output_db must be a decibel control"
        );
    }

    #[test]
    fn ir_output_db_default_seeds_from_first_capture_audit() {
        // The knob's default mirrors the engine's actual baseline so a
        // freshly created IR block (born at the first capture) shows the
        // real applied offset, not 0 dB. Volume invariant #10.
        let pkg = ir_package_with_capture_audit(Some(-22.9));
        let specs = synthesize_parameters_from_manifest(&pkg);
        let output_db = specs.iter().find(|s| s.path == "output_db").unwrap();
        assert_eq!(
            output_db.default_value,
            Some(domain::value_objects::ParameterValue::Float(-22.9)),
            "output_db default must be the first capture's audit baseline"
        );
    }

    fn nam_package_with_emoji_labels() -> LoadedPackage {
        // Real-world Bogner Ecstasy capture grid — `display_name` and
        // every `Text` value carry a leading emoji. Reproduces the
        // tofu/black-square symptom from issue #424.
        // Both cabinet values are capture-backed so the axis survives the
        // #649 dead-axis filter and the emoji stripping is exercised on a
        // rendered control.
        nam_amp_package(
            "nam_bogner_ecstasy",
            "Bogner Ecstasy",
            vec![GridParameter {
                name: "cabinet".into(),
                display_name: Some("📦 Cabinet".into()),
                values: vec![
                    ParameterValue::Text("✋ 4X12".into()),
                    ParameterValue::Text("🔥 2X12".into()),
                ],
            }],
            vec![
                GridCapture {
                    values: [(
                        "cabinet".to_string(),
                        ParameterValue::Text("✋ 4X12".into()),
                    )]
                    .into_iter()
                    .collect(),
                    file: "4x12.nam".into(),
                    output_gain_db: None,
                    noise_gate: None,
                },
                GridCapture {
                    values: [(
                        "cabinet".to_string(),
                        ParameterValue::Text("🔥 2X12".into()),
                    )]
                    .into_iter()
                    .collect(),
                    file: "2x12.nam".into(),
                    output_gain_db: None,
                    noise_gate: None,
                },
            ],
        )
    }

    #[test]
    fn nam_grid_parameter_label_strips_emoji_for_ui() {
        // Issue #424: shipped fonts (Bebas Neue, Inter, Permanent
        // Marker, …) carry no emoji glyphs; macOS cascades to Apple
        // Color Emoji, Windows / Linux do not, so emojis render as
        // tofu in the BlockEditorPanel selectors.
        let pkg = nam_package_with_emoji_labels();
        let specs = synthesize_parameters_from_manifest(&pkg);
        let cabinet = specs
            .iter()
            .find(|s| s.path == "cabinet")
            .expect("cabinet axis must be in synthesized schema");
        assert_eq!(
            cabinet.label, "Cabinet",
            "axis display_name must be emoji-free for UI rendering"
        );
        let block_core::param::ParameterDomain::Enum { options } = &cabinet.domain else {
            panic!(
                "text-valued grid axis must become an enum, got {:?}",
                cabinet.domain
            );
        };
        let labels: Vec<&str> = options.iter().map(|o| o.label.as_str()).collect();
        assert_eq!(
            labels,
            vec!["4X12", "2X12"],
            "option labels must be emoji-free; raw values stay for capture lookup"
        );
        // Pinned: storage-side values keep the original strings so
        // `resolve_capture` can still match user selections to the
        // manifest's `captures[].values`.
        let values: Vec<&str> = options.iter().map(|o| o.value.as_str()).collect();
        assert_eq!(
            values,
            vec!["✋ 4X12", "🔥 2X12"],
            "raw values must be preserved for capture lookup / persistence"
        );
    }
}
