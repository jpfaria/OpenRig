//! Generic LV2 instantiation from a `plugin_loader::LoadedPackage`.
//!
//! Replaces the per-plugin hard-coded port indices that used to live in
//! each `block-*::lv2_*.rs` reference file. Port discovery happens by
//! scanning the bundle's `<plugin>.ttl` (see
//! [`plugin_loader::dispatch::scan_lv2_ports`]); control values come
//! from the user's `ParameterSet` keyed by LV2 symbol with TTL defaults
//! as fallback.
//!
//! Issue: #287

use std::collections::BTreeSet;
use std::path::PathBuf;

use anyhow::{anyhow, bail, Result};
use block_core::param::ParameterSet;
use block_core::{
    wrap_with_output_gain_db, AudioChannelLayout, BlockProcessor, MonoProcessor, StereoProcessor,
};
use plugin_loader::dispatch::{lv2_control_value, scan_lv2_ports, Lv2Port, Lv2PortRole};
use plugin_loader::manifest::{Backend, Lv2Slot};
use plugin_loader::LoadedPackage;

use crate::{build_lv2_processor_full, build_stereo_lv2_processor_full};

/// Result of partitioning a plugin's scanned ports by role.
///
/// `extra_out` holds output **control** ports (gain-reduction meters,
/// latency indicators, etc.). LV2 requires every port to be connected
/// before `run()`; an unconnected output control port makes the plugin
/// write to null/garbage memory → SIGSEGV (issue #457). They are routed
/// to a scratch buffer just like surplus audio outputs.
#[derive(Debug, Default, PartialEq)]
struct PortPlan {
    audio_in: Vec<usize>,
    audio_out: Vec<usize>,
    /// `(port_index, initial_value)` for input control ports.
    control: Vec<(usize, f32)>,
    /// Atom/MIDI ports (in and out, deduplicated and sorted).
    atom: Vec<usize>,
    /// Output control ports — connected to a dummy buffer, never read.
    extra_out: Vec<usize>,
}

/// Partition scanned LV2 ports into the buckets the processor builders
/// expect. Pure (no FFI/IO) so the role→bucket mapping is unit-tested.
fn plan_ports(ports: &[Lv2Port], params: &ParameterSet) -> PortPlan {
    let indices = |role: Lv2PortRole| -> Vec<usize> {
        ports
            .iter()
            .filter(|p| p.role == role)
            .map(|p| p.index)
            .collect()
    };

    let control = ports
        .iter()
        .filter(|p| p.role == Lv2PortRole::ControlIn)
        .map(|p| {
            (
                p.index,
                lv2_control_value(&p.symbol, p.default_value, params),
            )
        })
        .collect();

    let mut atom: Vec<usize> = indices(Lv2PortRole::AtomIn)
        .into_iter()
        .chain(indices(Lv2PortRole::AtomOut))
        .collect();
    atom.sort_unstable();
    atom.dedup();

    PortPlan {
        audio_in: indices(Lv2PortRole::AudioIn),
        audio_out: indices(Lv2PortRole::AudioOut),
        control,
        atom,
        extra_out: indices(Lv2PortRole::ControlOut),
    }
}

/// Pre-flight safety net: every port the TTL scanner found MUST be in a
/// connected bucket before the plugin runs. LV2 requires all ports
/// connected before `run()`; an unconnected one makes the plugin write
/// to null/garbage memory → SIGSEGV, which no `catch_unwind` can catch
/// because it is a hardware fault inside foreign C code (issue #457).
///
/// Returning `Err` here converts that unrecoverable process crash into a
/// graceful "block failed to load" — the app stays alive. This guards
/// the #457 fix against future regressions (e.g. a new port role added
/// to the scanner but not bucketed in [`plan_ports`]).
///
/// It does NOT see ports the TTL scanner skipped or could not classify
/// (`scan_lv2_ports` drops `Other`): catching those needs the plugin's
/// real port count from the host, which the bare-ABI loader doesn't
/// expose — that is the out-of-process-sandbox boundary, out of scope.
fn assert_all_ports_connected(ports: &[Lv2Port], plan: &PortPlan, plugin_id: &str) -> Result<()> {
    let mut connected: BTreeSet<usize> = BTreeSet::new();
    connected.extend(&plan.audio_in);
    connected.extend(&plan.audio_out);
    connected.extend(plan.control.iter().map(|(idx, _)| *idx));
    connected.extend(&plan.atom);
    connected.extend(&plan.extra_out);

    let orphans: Vec<String> = ports
        .iter()
        .filter(|p| !connected.contains(&p.index))
        .map(|p| format!("port {} `{}` ({:?})", p.index, p.symbol, p.role))
        .collect();

    if !orphans.is_empty() {
        bail!(
            "LV2 plugin `{plugin_id}` has {} unconnectable port(s) — refusing \
             to load to avoid a SIGSEGV on run(): {}",
            orphans.len(),
            orphans.join(", ")
        );
    }
    Ok(())
}

/// Build a [`BlockProcessor`] from a disk-backed LV2 package.
pub fn build_from_package(
    package: &LoadedPackage,
    params: &ParameterSet,
    sample_rate: f32,
    layout: AudioChannelLayout,
) -> Result<BlockProcessor> {
    let (plugin_uri, lib_path) = match &package.manifest.backend {
        Backend::Lv2 {
            plugin_uri,
            binaries,
        } => {
            let slot = current_slot()?;
            let rel = binaries.get(&slot).ok_or_else(|| {
                anyhow!(
                    "LV2 plugin `{}` ships no binary for current platform slot {:?}",
                    package.manifest.id,
                    slot
                )
            })?;
            (plugin_uri.clone(), package.root.join(rel))
        }
        _ => bail!(
            "lv2::build_from_package called with non-LV2 backend (model `{}`)",
            package.manifest.id
        ),
    };
    // Prefer the shared `<package>/data/` bundle (deduplicated TTLs);
    // fall back to the legacy per-platform layout where TTLs sat next
    // to the binary.
    let data_dir = package.root.join("data");
    let bundle_path = if data_dir.is_dir() {
        data_dir
    } else {
        lib_path
            .parent()
            .ok_or_else(|| anyhow!("LV2 binary `{lib_path:?}` has no parent directory"))?
            .to_path_buf()
    };

    let ports = scan_lv2_ports(&bundle_path, &plugin_uri)?;
    let plan = plan_ports(&ports, params);
    assert_all_ports_connected(&ports, &plan, &package.manifest.id)?;

    let lib_str = path_str(&lib_path)?;
    let bundle_str = path_str(&bundle_path)?;
    let sr = sample_rate as f64;

    let processor = match (plan.audio_in.len(), plan.audio_out.len()) {
        (1, 1) | (1, 2) => build_mono_input(
            &lib_str,
            &plugin_uri,
            sr,
            &bundle_str,
            &plan,
            layout,
            &package.manifest.id,
        )?,
        (2, 2) => build_stereo_input(
            &lib_str,
            &plugin_uri,
            sr,
            &bundle_str,
            &plan,
            layout,
            &package.manifest.id,
        )?,
        (a_in, a_out) => bail!(
            "LV2 plugin `{}` has unsupported audio shape: {a_in} in / {a_out} out",
            package.manifest.id
        ),
    };
    // Issue #491: aplica `manifest.output_gain_db` (baseline objetivo do
    // audit, em dB) como wrapper estático pós-process. NAM faz isso via
    // `plugin_params.output_level_db`; LV2 não tem level shift embutido,
    // então um wrapper estático é o caminho mais simples. Na prática os
    // manifests LV2 não carregam o campo (calibração é só NAM), então é
    // no-op — mantido por consistência de contrato.
    Ok(wrap_with_output_gain_db(
        processor,
        package.manifest.output_gain_db,
    ))
}

// FFI builder: threads plugin location (lib/uri/sr/bundle) + the full
// port plan + chain layout into the processor. Bundling these into a
// struct would not reduce real coupling — kept flat like the rest of
// this file's builders.
#[allow(clippy::too_many_arguments)]
fn build_mono_input(
    lib_path: &str,
    uri: &str,
    sample_rate: f64,
    bundle_path: &str,
    plan: &PortPlan,
    layout: AudioChannelLayout,
    plugin_id: &str,
) -> Result<BlockProcessor> {
    // Lv2Processor implements MonoProcessor (single sample in, single
    // sample out). Wrap into the requested layout — for stereo we
    // duplicate the processor across L/R (DualMono) so a 1-out plugin
    // also satisfies stereo chains. `build_lv2_processor_full` connects
    // atom + output-control ports; empty slices reduce to the plain case.
    let make = || -> Result<crate::Lv2Processor> {
        build_lv2_processor_full(
            lib_path,
            uri,
            sample_rate,
            bundle_path,
            &plan.audio_in,
            &plan.audio_out,
            &plan.control,
            &plan.atom,
            &plan.extra_out,
        )
    };
    let _ = plugin_id;
    match layout {
        AudioChannelLayout::Mono => Ok(BlockProcessor::Mono(Box::new(make()?))),
        AudioChannelLayout::Stereo => {
            let left = make()?;
            let right = make()?;
            Ok(BlockProcessor::Stereo(Box::new(DualMonoLv2 {
                left,
                right,
            })))
        }
    }
}

#[allow(clippy::too_many_arguments)] // see build_mono_input
fn build_stereo_input(
    lib_path: &str,
    uri: &str,
    sample_rate: f64,
    bundle_path: &str,
    plan: &PortPlan,
    layout: AudioChannelLayout,
    plugin_id: &str,
) -> Result<BlockProcessor> {
    let make = || -> Result<crate::StereoLv2Processor> {
        build_stereo_lv2_processor_full(
            lib_path,
            uri,
            sample_rate,
            bundle_path,
            &plan.audio_in,
            &plan.audio_out,
            &plan.control,
            &plan.atom,
            &plan.extra_out,
        )
    };
    let _ = plugin_id;
    match layout {
        AudioChannelLayout::Stereo => Ok(BlockProcessor::Stereo(Box::new(make()?))),
        AudioChannelLayout::Mono => {
            // Stereo plugin in a mono chain: feed both inputs from the
            // mono sample and average the outputs.
            Ok(BlockProcessor::Mono(Box::new(StereoAsMono {
                inner: make()?,
            })))
        }
    }
}

fn current_slot() -> Result<Lv2Slot> {
    #[cfg(target_os = "macos")]
    {
        Ok(Lv2Slot::MacosUniversal)
    }
    #[cfg(all(target_os = "linux", target_arch = "x86_64"))]
    {
        Ok(Lv2Slot::LinuxX86_64)
    }
    #[cfg(all(target_os = "linux", target_arch = "aarch64"))]
    {
        Ok(Lv2Slot::LinuxAarch64)
    }
    #[cfg(all(target_os = "windows", target_arch = "x86_64"))]
    {
        Ok(Lv2Slot::WindowsX86_64)
    }
    #[cfg(all(target_os = "windows", target_arch = "aarch64"))]
    {
        Ok(Lv2Slot::WindowsAarch64)
    }
    #[cfg(not(any(
        target_os = "macos",
        all(target_os = "linux", target_arch = "x86_64"),
        all(target_os = "linux", target_arch = "aarch64"),
        all(target_os = "windows", target_arch = "x86_64"),
        all(target_os = "windows", target_arch = "aarch64"),
    )))]
    {
        Err(anyhow!("no LV2 slot available for current platform"))
    }
}

fn path_str(p: &PathBuf) -> Result<String> {
    p.to_str()
        .map(|s| s.to_string())
        .ok_or_else(|| anyhow!("non-utf8 path: {p:?}"))
}

struct DualMonoLv2 {
    left: crate::Lv2Processor,
    right: crate::Lv2Processor,
}

impl StereoProcessor for DualMonoLv2 {
    fn process_frame(&mut self, input: [f32; 2]) -> [f32; 2] {
        [
            self.left.process_sample(input[0]),
            self.right.process_sample(input[1]),
        ]
    }
}

struct StereoAsMono {
    inner: crate::StereoLv2Processor,
}

impl MonoProcessor for StereoAsMono {
    fn process_sample(&mut self, input: f32) -> f32 {
        let [l, r] = self.inner.process_frame([input, input]);
        0.5 * (l + r)
    }
}

/// Register this crate's builder in the global package-builders table.
/// Called once at process startup so `LoadedPackage::build_processor`
/// can dispatch `Backend::Lv2` packages without the caller having to
/// know about the lv2 crate.
pub fn register_builder() {
    plugin_loader::package_builders::register(
        plugin_loader::package_builders::BackendKind::Lv2,
        build_from_package,
    );
}

#[cfg(test)]
#[path = "from_package_tests.rs"]
mod tests;
