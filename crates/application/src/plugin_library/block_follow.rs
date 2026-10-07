//! Responsibility: keeps the blocks of a plugin on their capture across a save.

use domain::ids::BlockId;
use plugin_loader::manifest::PluginManifest;
use project::block::grid_follow::follow_capture;
use project::block::grid_version_follow::capture_grid;
use project::block::{for_each_block_mut, AudioBlock, AudioBlockKind};
use project::param::ParameterSet;

/// Moves every block in `blocks` that plays `from` (paths included) to the
/// values that pick the same capture file in `to`; select options too. Returns the blocks that
/// changed; a block whose values pick no capture is left alone.
pub fn follow_blocks(
    blocks: &mut [AudioBlock],
    from: &PluginManifest,
    to: &PluginManifest,
) -> Vec<BlockId> {
    let (Some((from_parameters, from_captures)), Some((to_parameters, to_captures))) =
        (capture_grid(from), capture_grid(to))
    else {
        return Vec::new();
    };
    let mut changed = Vec::new();
    for_each_block_mut(blocks, &mut |block| {
        if let AudioBlockKind::Select(select) = &mut block.kind {
            changed.extend(follow_blocks(&mut select.options, from, to));
            return;
        }
        let id = block.id.clone();
        let Some(params) = plugin_params(block, &from.id) else {
            return;
        };
        if let Some(moved) = follow_capture(
            from_parameters,
            from_captures,
            to_parameters,
            to_captures,
            params,
        ) {
            if moved != *params {
                *params = moved;
                changed.push(id);
            }
        }
    });
    changed
}

fn plugin_params<'a>(block: &'a mut AudioBlock, plugin_id: &str) -> Option<&'a mut ParameterSet> {
    match &mut block.kind {
        AudioBlockKind::Core(core) if core.model == plugin_id => Some(&mut core.params),
        AudioBlockKind::Nam(nam) if nam.model == plugin_id => Some(&mut nam.params),
        _ => None,
    }
}
