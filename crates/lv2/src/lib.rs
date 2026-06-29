// Snapshot of complexity debt that existed on develop before the
// #548 build break was fixed (issue #576). Refactor of long fns and
// complex types is tracked under god-file ticket #276 and follow-ups.
// Allowing crate-wide keeps the QG honest about NEW regressions
// instead of perpetually re-reporting the existing snapshot.
#![allow(clippy::too_many_lines)]
#![allow(clippy::cognitive_complexity)]
#![allow(clippy::too_many_arguments)]
#![allow(clippy::type_complexity)]

mod from_package;
mod host;
mod processor;
mod stereo_processor;

pub use from_package::{build_from_package, register_builder};
pub use host::{Lv2Plugin, Lv2PortInfo, Lv2PortKind};
pub use host::{issue670_schedule_work_thread_check, WorkerThreadCheck};
pub use processor::Lv2Processor;
pub use stereo_processor::StereoLv2Processor;

use anyhow::Result;

/// Build a mono LV2 processor connecting audio, control, atom AND
/// output-control ports.
///
/// - `lib_path`: full path to the plugin shared library (`.dylib`/`.so`/`.dll`)
/// - `uri`: LV2 plugin URI
/// - `sample_rate`: audio sample rate
/// - `bundle_path`: path to the `.lv2` bundle directory containing TTL metadata
/// - `audio_in_ports` / `audio_out_ports`: audio port indices
/// - `control_ports`: `(port_index, initial_value)` for input control ports
/// - `atom_ports`: atom/MIDI sidechain ports (connected to an empty buffer)
/// - `extra_out_ports`: output control ports (meters, latency) connected to
///   a scratch buffer so the plugin never writes to unconnected memory on
///   `run()` (issue #457). Empty atom/extra slices reduce this to the
///   plain audio+control case — this is the single entry point so no
///   caller can accidentally skip a port and reintroduce the crash.
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

/// Build a stereo LV2 processor with atom ports AND extra output ports.
///
/// `extra_out_ports` (output control ports — meters, latency) are
/// connected to a scratch buffer so the plugin never writes to
/// unconnected memory on `run()` (issue #457). Empty atom/extra slices
/// reduce this to the plain stereo case.
pub fn build_stereo_lv2_processor_full(
    lib_path: &str,
    uri: &str,
    sample_rate: f64,
    bundle_path: &str,
    audio_in_ports: &[usize],
    audio_out_ports: &[usize],
    control_ports: &[(usize, f32)],
    atom_ports: &[usize],
    extra_out_ports: &[usize],
) -> Result<StereoLv2Processor> {
    let plugin = Lv2Plugin::load(lib_path, uri, sample_rate, bundle_path)?;
    Ok(StereoLv2Processor::with_extra_ports(
        plugin,
        audio_in_ports,
        audio_out_ports,
        control_ports,
        atom_ports,
        extra_out_ports,
    ))
}

// LV2 path resolvers (resolve_lv2_lib, resolve_lv2_bundle, default_lv2_lib_dir)
// removed in issue #287: their only callers were the per-plugin
// `lv2_*.rs` files in crates/block-*/src/, which moved to OpenRig-plugins.
// Plugin-loader handles binary/bundle path resolution via the package root
// in each loaded manifest now.
