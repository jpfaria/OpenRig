//! Load-time helper that disables blocks whose model cannot be resolved
//! on this machine right now.
//!
//! Issue #606: a project may reference a disk-package model (a NAM/IR/LV2
//! pack) that is not installed in the user's plugin catalog — e.g. the pack
//! was never extracted into `plugins/source`, or the project came from
//! another machine. Such a block cannot be built; the engine bypasses it as
//! faulted (#574) so the chain keeps playing, but the GUI still shows it as
//! "on", which is confusing.
//!
//! On load we flip every unresolvable block to `enabled = false` so the
//! pedal is visibly deactivated and the chain plays without it. The model
//! is preserved: when the missing pack is installed and the catalog
//! reloads, [`crate::catalog::is_model_available`] reports it available
//! again and the user can re-enable the block.
//!
//! Routing-only kinds (Input/Output/Insert) and the composite Select kind
//! have no single resolvable model and are never touched here.

use crate::block::AudioBlockKind;
use crate::project::Project;
use domain::ids::BlockId;

/// Disable every currently-enabled block whose model is unavailable.
/// Returns the ids of the blocks that were flipped off (already-disabled
/// blocks are left as-is and not reported).
pub fn disable_unavailable_blocks(project: &mut Project) -> Vec<BlockId> {
    let mut disabled = Vec::new();
    for chain in &mut project.chains {
        for block in &mut chain.blocks {
            if !block.enabled || block_model_is_available(&block.kind) {
                continue;
            }
            block.enabled = false;
            disabled.push(block.id.clone());
        }
    }
    disabled
}

/// True if the block's model resolves to a buildable processor right now.
/// Mirrors the runtime build's resolution: native registry or catalog for
/// `Core`, the plugin catalog for the disk-backed `Nam` kind.
///
/// Shared single source of truth for "can this block be built": the load
/// pass above uses it to disable unavailable blocks, and the
/// `ToggleBlockEnabled` command uses it to refuse enabling one (#606).
pub fn block_model_is_available(kind: &AudioBlockKind) -> bool {
    match kind {
        AudioBlockKind::Core(core) => {
            crate::catalog::is_model_available(&core.effect_type, &core.model)
        }
        AudioBlockKind::Nam(nam) => plugin_loader::registry::find(&nam.model).is_some(),
        // Routing-only and composite kinds carry no single resolvable model.
        AudioBlockKind::Select(_)
        | AudioBlockKind::Input(_)
        | AudioBlockKind::Output(_)
        | AudioBlockKind::Insert(_) => true,
    }
}
