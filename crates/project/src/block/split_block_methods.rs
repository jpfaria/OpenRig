//! Responsibility: states the structural rules a chain's split obeys.
//!
//! #328 (spec §1.1): a chain holds at most one Mix split and at most one Y
//! split; the Y ends the chain (only the chain's own `Input`/`Output` ports may
//! follow it), so a Mix can only come before it; a path holds processing
//! blocks only — no split, select or port — so nesting stays one level deep.

use super::split_block::{SplitBlock, SplitEnd};
use super::split_lookup::{find_split_with_end, splits};
use super::types::{AudioBlock, AudioBlockKind};

impl SplitBlock {
    /// The path rule: every block of `a` and `b` is a processing block.
    pub fn validate_structure(&self) -> Result<(), String> {
        for block in self.a.iter().chain(&self.b) {
            if matches!(
                block.kind,
                AudioBlockKind::Split(_)
                    | AudioBlockKind::Select(_)
                    | AudioBlockKind::Input(_)
                    | AudioBlockKind::Output(_)
                    | AudioBlockKind::Insert(_)
            ) {
                return Err(format!(
                    "split path block '{}' is a {} block; a path holds processing blocks only",
                    block.id.0,
                    block.kind.label()
                ));
            }
        }
        Ok(())
    }
}

/// The chain-level rules: at most one split of each end, and nothing but the
/// chain's own `Input`/`Output` ports after the Y split.
pub fn validate_split_layout(blocks: &[AudioBlock]) -> Result<(), String> {
    for (end, name) in [(SplitEnd::Mix, "Mix"), (SplitEnd::Y, "Y")] {
        let count = splits(blocks).filter(|(_, split)| split.end == end).count();
        if count > 1 {
            return Err(format!(
                "a chain holds at most one split of each end (a Mix, then a Y), found {count} {name}"
            ));
        }
    }
    let Some((position, _)) = find_split_with_end(blocks, SplitEnd::Y) else {
        return Ok(());
    };
    let follower = blocks[position + 1..].iter().find(|block| {
        !matches!(
            block.kind,
            AudioBlockKind::Input(_) | AudioBlockKind::Output(_)
        )
    });
    if let Some(block) = follower {
        return Err(format!(
            "a Y split ends the chain, but block '{}' follows it",
            block.id.0
        ));
    }
    Ok(())
}
