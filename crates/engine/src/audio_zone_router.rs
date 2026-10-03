//! Responsibility: sends each allocation of the process to the audio zone or the system zone.
//!
//! OpenRig wired its whole private memory to keep the audio resident, so
//! every byte the UI, the YAML loader or a rebuild ever freed
//! stayed in RAM for good. Here the process gets a second malloc zone, the
//! audio zone, and a router zone ahead of the system one: an allocation made
//! by a thread marked by `audio_alloc_scope` lands in the audio zone, any
//! other in the system zone, and a free goes back to the zone that owns the
//! pointer. This covers Rust, C and C++ alike — the plugins (NAM, LV2, VST3)
//! call `malloc` themselves. Only the audio zone is then wired.
//!
//! macOS only, like the wiring. Elsewhere [`install`] reports `false` and
//! nothing changes.

#[cfg(target_os = "macos")]
#[path = "audio_zone_router_macos.rs"]
mod imp;

#[cfg(target_os = "macos")]
pub(crate) use imp::{audio_zone_address, MallocZone};
#[cfg(target_os = "macos")]
pub use imp::{install, is_audio_allocation};

/// Installs the router once per process. `false`: the audio memory cannot be
/// told apart on this platform (or the system refused), and callers keep
/// treating the whole process as audio.
#[cfg(not(target_os = "macos"))]
pub fn install() -> bool {
    false
}

/// Whether `ptr` was allocated in the audio zone.
#[cfg(not(target_os = "macos"))]
pub fn is_audio_allocation(_ptr: *const u8) -> bool {
    false
}

#[cfg(all(test, target_os = "macos"))]
#[path = "audio_zone_router_tests.rs"]
mod audio_zone_router_tests;
