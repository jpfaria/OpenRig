mod host;
mod processor;

pub use host::{Lv2Plugin, Lv2PortInfo, Lv2PortKind};
pub use processor::Lv2Processor;

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
