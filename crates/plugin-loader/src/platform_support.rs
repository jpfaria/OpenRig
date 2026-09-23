//! Responsibility: says whether a package ships what a platform needs to run it.

use crate::discover::LoadedPackage;
use crate::manifest::{Backend, Lv2Slot};
use crate::package::current_platform_slot;

/// `true` when `package` can be built on the platform `slot` names (`None`:
/// a platform no binary slot covers).
///
/// Discovery keeps a package that has no binary for the host (#477): each OS
/// package strips the other platforms' binaries, so a missing slot is not
/// corruption. But such a package cannot be built here, so the catalog must
/// not offer it (#978: on Windows, 51 bundled LV2 and every bundled VST3).
pub fn package_runs_on(package: &LoadedPackage, slot: Option<Lv2Slot>) -> bool {
    match &package.manifest.backend {
        Backend::Lv2 { binaries, .. } => slot.is_some_and(|slot| binaries.contains_key(&slot)),
        Backend::Vst3 { bundle, .. } => slot.and_then(vst3_binary_dir).is_some_and(|dir| {
            package
                .root
                .join(bundle)
                .join("Contents")
                .join(dir)
                .is_dir()
        }),
        Backend::Native { .. } | Backend::Nam { .. } | Backend::Ir { .. } => true,
    }
}

/// [`package_runs_on`] for the platform this binary was built for.
pub fn package_runs_here(package: &LoadedPackage) -> bool {
    package_runs_on(package, current_platform_slot())
}

/// The `Contents/<dir>` a `.vst3` bundle keeps its binary in, per the VST3
/// bundle format.
fn vst3_binary_dir(slot: Lv2Slot) -> Option<&'static str> {
    Some(match slot {
        Lv2Slot::MacosUniversal => "MacOS",
        Lv2Slot::WindowsX86_64 => "x86_64-win",
        Lv2Slot::WindowsAarch64 => "arm64-win",
        Lv2Slot::LinuxX86_64 => "x86_64-linux",
        Lv2Slot::LinuxAarch64 => "aarch64-linux",
    })
}

#[cfg(test)]
#[path = "platform_support_tests.rs"]
mod tests;
