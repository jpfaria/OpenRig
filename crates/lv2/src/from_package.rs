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

use std::path::PathBuf;

use anyhow::{anyhow, bail, Result};
use block_core::param::ParameterSet;
use block_core::{
    AudioChannelLayout, BlockProcessor, MonoProcessor, StereoProcessor,
};
use plugin_loader::dispatch::{lv2_control_value, scan_lv2_ports, Lv2PortRole};
use plugin_loader::manifest::{Backend, Lv2Slot};
use plugin_loader::LoadedPackage;

use crate::{
    build_lv2_processor, build_lv2_processor_with_atoms, build_stereo_lv2_processor,
    build_stereo_lv2_processor_with_atoms,
};

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
    let bundle_path = lib_path
        .parent()
        .ok_or_else(|| anyhow!("LV2 binary `{lib_path:?}` has no parent directory"))?
        .to_path_buf();

    let ports = scan_lv2_ports(&bundle_path, &plugin_uri)?;

    let audio_in: Vec<usize> = ports
        .iter()
        .filter(|p| p.role == Lv2PortRole::AudioIn)
        .map(|p| p.index)
        .collect();
    let audio_out: Vec<usize> = ports
        .iter()
        .filter(|p| p.role == Lv2PortRole::AudioOut)
        .map(|p| p.index)
        .collect();
    let atom_in: Vec<usize> = ports
        .iter()
        .filter(|p| p.role == Lv2PortRole::AtomIn)
        .map(|p| p.index)
        .collect();
    let atom_out: Vec<usize> = ports
        .iter()
        .filter(|p| p.role == Lv2PortRole::AtomOut)
        .map(|p| p.index)
        .collect();
    let control_ports: Vec<(usize, f32)> = ports
        .iter()
        .filter(|p| p.role == Lv2PortRole::ControlIn)
        .map(|p| (p.index, lv2_control_value(&p.symbol, p.default_value, params)))
        .collect();
    let mut atom_ports: Vec<usize> = atom_in.iter().chain(atom_out.iter()).copied().collect();
    atom_ports.sort_unstable();
    atom_ports.dedup();

    let lib_str = path_str(&lib_path)?;
    let bundle_str = path_str(&bundle_path)?;
    let sr = sample_rate as f64;

    match (audio_in.len(), audio_out.len()) {
        (1, 1) | (1, 2) => build_mono_input(
            &lib_str,
            &plugin_uri,
            sr,
            &bundle_str,
            &audio_in,
            &audio_out,
            &control_ports,
            &atom_ports,
            layout,
            &package.manifest.id,
        ),
        (2, 2) => build_stereo_input(
            &lib_str,
            &plugin_uri,
            sr,
            &bundle_str,
            &audio_in,
            &audio_out,
            &control_ports,
            &atom_ports,
            layout,
            &package.manifest.id,
        ),
        (a_in, a_out) => bail!(
            "LV2 plugin `{}` has unsupported audio shape: {a_in} in / {a_out} out",
            package.manifest.id
        ),
    }
}

#[allow(clippy::too_many_arguments)]
fn build_mono_input(
    lib_path: &str,
    uri: &str,
    sample_rate: f64,
    bundle_path: &str,
    audio_in: &[usize],
    audio_out: &[usize],
    control_ports: &[(usize, f32)],
    atom_ports: &[usize],
    layout: AudioChannelLayout,
    plugin_id: &str,
) -> Result<BlockProcessor> {
    // Lv2Processor implements MonoProcessor (single sample in, single
    // sample out). Wrap into the requested layout — for stereo we
    // duplicate the processor across L/R (DualMono) so a 1-out plugin
    // also satisfies stereo chains.
    let make = || -> Result<crate::Lv2Processor> {
        if atom_ports.is_empty() {
            build_lv2_processor(
                lib_path,
                uri,
                sample_rate,
                bundle_path,
                audio_in,
                audio_out,
                control_ports,
            )
        } else {
            build_lv2_processor_with_atoms(
                lib_path,
                uri,
                sample_rate,
                bundle_path,
                audio_in,
                audio_out,
                control_ports,
                atom_ports,
            )
        }
    };
    let _ = plugin_id;
    match layout {
        AudioChannelLayout::Mono => Ok(BlockProcessor::Mono(Box::new(make()?))),
        AudioChannelLayout::Stereo => {
            let left = make()?;
            let right = make()?;
            Ok(BlockProcessor::Stereo(Box::new(DualMonoLv2 { left, right })))
        }
    }
}

#[allow(clippy::too_many_arguments)]
fn build_stereo_input(
    lib_path: &str,
    uri: &str,
    sample_rate: f64,
    bundle_path: &str,
    audio_in: &[usize],
    audio_out: &[usize],
    control_ports: &[(usize, f32)],
    atom_ports: &[usize],
    layout: AudioChannelLayout,
    plugin_id: &str,
) -> Result<BlockProcessor> {
    let make = || -> Result<crate::StereoLv2Processor> {
        if atom_ports.is_empty() {
            build_stereo_lv2_processor(
                lib_path,
                uri,
                sample_rate,
                bundle_path,
                audio_in,
                audio_out,
                control_ports,
            )
        } else {
            build_stereo_lv2_processor_with_atoms(
                lib_path,
                uri,
                sample_rate,
                bundle_path,
                audio_in,
                audio_out,
                control_ports,
                atom_ports,
            )
        }
    };
    let _ = plugin_id;
    match layout {
        AudioChannelLayout::Stereo => Ok(BlockProcessor::Stereo(Box::new(make()?))),
        AudioChannelLayout::Mono => {
            // Stereo plugin in a mono chain: feed both inputs from the
            // mono sample and average the outputs.
            Ok(BlockProcessor::Mono(Box::new(StereoAsMono { inner: make()? })))
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
