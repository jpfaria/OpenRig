//! Responsibility: answers what the loaded catalog contains.

use crate::discover::LoadedPackage;
use crate::manifest::{Backend, BlockType};
use crate::registry::REGISTRY;

/// Every plugin currently registered (natives + disk packages). Empty
/// until [`init`] / [`init_many`] / [`reload`] runs.
pub fn packages() -> &'static [LoadedPackage] {
    *REGISTRY.read().expect("REGISTRY poisoned")
}

/// Plugins whose manifest declares `block_type`. Returned in registration
/// order (natives first, then disk packages alphabetically by directory).
pub fn packages_for(block_type: BlockType) -> Vec<&'static LoadedPackage> {
    packages()
        .iter()
        .filter(|p| p.manifest.block_type == block_type)
        .collect()
}

/// Look up a single plugin by manifest id (`p.manifest.id`).
pub fn find(model_id: &str) -> Option<&'static LoadedPackage> {
    packages().iter().find(|p| p.manifest.id == model_id)
}

/// True if `model_id` resolves to a buildable processor, mirroring the
/// native-then-catalog resolution order every `build_*_processor_for_layout`
/// uses: a native model of its family that is available on this platform,
/// or a disk-package (NAM/IR/LV2/VST3) present in the catalog.
///
/// Issue #606: the per-family checks used to treat *any* non-native id as
/// available, so an uninstalled disk package slipped through to the native
/// registry and failed with a misleading "unsupported <family> model".
/// Routing the disk case through the catalog makes the block report
/// unavailable instead, so the caller can disable it and keep the chain
/// playing.
pub fn model_available(
    model_id: &str,
    is_native: impl Fn(&str) -> bool,
    native_available_on_platform: impl Fn(&str) -> bool,
) -> bool {
    if is_native(model_id) {
        native_available_on_platform(model_id)
    } else {
        find(model_id).is_some()
    }
}

/// Count of natives + disk packages currently in the catalog.
pub fn len() -> usize {
    packages().len()
}

/// Count of just the native plugins (entries whose backend is `Native`).
pub fn native_count() -> usize {
    packages()
        .iter()
        .filter(|p| matches!(p.manifest.backend, Backend::Native { .. }))
        .count()
}
