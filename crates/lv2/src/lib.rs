mod host;
mod processor;
mod stereo_processor;

pub use host::{Lv2Plugin, Lv2PortInfo, Lv2PortKind};
pub use processor::Lv2Processor;
pub use stereo_processor::StereoLv2Processor;

use anyhow::Result;

/// Build a ready-to-use mono LV2 processor.
///
/// - `lib_path`: full path to the plugin shared library (`.dylib`/`.so`/`.dll`)
/// - `uri`: LV2 plugin URI
/// - `sample_rate`: audio sample rate
/// - `bundle_path`: path to the `.lv2` bundle directory containing TTL metadata
/// - `audio_in_ports`: port indices for audio inputs
/// - `audio_out_ports`: port indices for audio outputs
/// - `control_ports`: `(port_index, initial_value)` pairs for control ports
pub fn build_lv2_processor(
    lib_path: &str,
    uri: &str,
    sample_rate: f64,
    bundle_path: &str,
    audio_in_ports: &[usize],
    audio_out_ports: &[usize],
    control_ports: &[(usize, f32)],
) -> Result<Lv2Processor> {
    let plugin = Lv2Plugin::load(lib_path, uri, sample_rate, bundle_path)?;
    Ok(Lv2Processor::new(
        plugin,
        audio_in_ports,
        audio_out_ports,
        control_ports,
    ))
}

/// Build a mono LV2 processor with extra (dummy) output ports connected.
///
/// Use for plugins with more outputs than you read (e.g., mono-in/stereo-out
/// used as mono). The extra ports are connected to a scratch buffer.
pub fn build_lv2_processor_with_extras(
    lib_path: &str,
    uri: &str,
    sample_rate: f64,
    bundle_path: &str,
    audio_in_ports: &[usize],
    audio_out_ports: &[usize],
    control_ports: &[(usize, f32)],
    extra_out_ports: &[usize],
) -> Result<Lv2Processor> {
    build_lv2_processor_full(lib_path, uri, sample_rate, bundle_path, audio_in_ports, audio_out_ports, control_ports, &[], extra_out_ports)
}

/// Build a mono LV2 processor with atom ports AND extra output ports.
pub fn build_lv2_processor_full(
    lib_path: &str,
    uri: &str,
    sample_rate: f64,
    bundle_path: &str,
    audio_in_ports: &[usize],
    audio_out_ports: &[usize],
    control_ports: &[(usize, f32)],
    atom_ports: &[usize],
    extra_out_ports: &[usize],
) -> Result<Lv2Processor> {
    let plugin = Lv2Plugin::load(lib_path, uri, sample_rate, bundle_path)?;
    Ok(Lv2Processor::with_extra_ports(
        plugin,
        audio_in_ports,
        audio_out_ports,
        control_ports,
        atom_ports,
        extra_out_ports,
    ))
}

/// Build a mono LV2 processor with atom/MIDI sidechain ports connected.
///
/// Use this for plugins that have MIDI atom input ports (like pitch correction
/// plugins) that need a valid buffer even when unused.
pub fn build_lv2_processor_with_atoms(
    lib_path: &str,
    uri: &str,
    sample_rate: f64,
    bundle_path: &str,
    audio_in_ports: &[usize],
    audio_out_ports: &[usize],
    control_ports: &[(usize, f32)],
    atom_ports: &[usize],
) -> Result<Lv2Processor> {
    let plugin = Lv2Plugin::load(lib_path, uri, sample_rate, bundle_path)?;
    Ok(Lv2Processor::with_atom_ports(
        plugin,
        audio_in_ports,
        audio_out_ports,
        control_ports,
        atom_ports,
    ))
}

/// Build a ready-to-use stereo LV2 processor (2-in / 2-out).
pub fn build_stereo_lv2_processor(
    lib_path: &str,
    uri: &str,
    sample_rate: f64,
    bundle_path: &str,
    audio_in_ports: &[usize],
    audio_out_ports: &[usize],
    control_ports: &[(usize, f32)],
) -> Result<StereoLv2Processor> {
    let plugin = Lv2Plugin::load(lib_path, uri, sample_rate, bundle_path)?;
    Ok(StereoLv2Processor::new(
        plugin,
        audio_in_ports,
        audio_out_ports,
        control_ports,
    ))
}

/// Build a stereo LV2 processor with atom/MIDI ports connected.
pub fn build_stereo_lv2_processor_with_atoms(
    lib_path: &str,
    uri: &str,
    sample_rate: f64,
    bundle_path: &str,
    audio_in_ports: &[usize],
    audio_out_ports: &[usize],
    control_ports: &[(usize, f32)],
    atom_ports: &[usize],
) -> Result<StereoLv2Processor> {
    let plugin = Lv2Plugin::load(lib_path, uri, sample_rate, bundle_path)?;
    Ok(StereoLv2Processor::with_atom_ports(
        plugin,
        audio_in_ports,
        audio_out_ports,
        control_ports,
        atom_ports,
    ))
}

/// Platform-specific default directory for prebuilt LV2 shared libraries,
/// relative to the project root.
pub fn default_lv2_lib_dir() -> &'static str {
    #[cfg(target_os = "macos")]
    {
        "libs/lv2/macos-universal"
    }
    #[cfg(all(target_os = "linux", target_arch = "x86_64"))]
    {
        "libs/lv2/linux-x86_64"
    }
    #[cfg(all(target_os = "linux", target_arch = "aarch64"))]
    {
        "libs/lv2/linux-aarch64"
    }
    #[cfg(all(target_os = "windows", target_arch = "x86_64"))]
    {
        "libs/lv2/windows-x64"
    }
    #[cfg(all(target_os = "windows", target_arch = "aarch64"))]
    {
        "libs/lv2/windows-arm64"
    }
}

/// Resolve the full filesystem path to an LV2 shared library binary.
///
/// Searches relative to the executable (`../../<lv2_libs>/<binary>`) first,
/// then falls back to treating `lv2_libs` as a standalone path.
pub fn resolve_lv2_lib(binary_name: &str) -> Result<String> {
    let paths = infra_filesystem::asset_paths();
    let exe_dir = std::env::current_exe()
        .ok()
        .and_then(|p| p.parent().map(|p| p.to_path_buf()));
    let candidates = [
        exe_dir
            .as_ref()
            .map(|d| d.join("../../").join(&paths.lv2_libs).join(binary_name)),
        Some(std::path::PathBuf::from(&paths.lv2_libs).join(binary_name)),
    ];
    for candidate in candidates.iter().flatten() {
        if candidate.exists() {
            return Ok(candidate.to_string_lossy().to_string());
        }
    }
    anyhow::bail!(
        "LV2 binary '{}' not found in '{}'",
        binary_name,
        paths.lv2_libs
    )
}

/// Resolve the full filesystem path to an LV2 bundle directory.
///
/// Searches relative to the executable (`../../<lv2_plugins>/<bundle>`)
/// first, then falls back to treating `lv2_plugins` as a standalone path.
pub fn resolve_lv2_bundle(bundle_dir: &str) -> Result<String> {
    let paths = infra_filesystem::asset_paths();
    let exe_dir = std::env::current_exe()
        .ok()
        .and_then(|p| p.parent().map(|p| p.to_path_buf()));
    let candidates = [
        exe_dir
            .as_ref()
            .map(|d| d.join("../../").join(&paths.lv2_plugins).join(bundle_dir)),
        Some(std::path::PathBuf::from(&paths.lv2_plugins).join(bundle_dir)),
    ];
    for candidate in candidates.iter().flatten() {
        if candidate.exists() {
            return Ok(candidate.to_string_lossy().to_string());
        }
    }
    anyhow::bail!(
        "LV2 bundle '{}' not found in '{}'",
        bundle_dir,
        paths.lv2_plugins
    )
}
