//! Responsibility: states the structural rules a chain's split obeys.
//!
//! #328 (spec §1.1): at most one split per chain; a path holds processing
//! blocks only — no split, select or port — so nesting stays one level deep;
//! and a Y split ends the chain (only the chain's own `Input`/`Output` ports
//! may follow it).

use super::split_block::{SplitBlock, SplitEnd};
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

/// The chain's split and its position in `blocks`, if it has one.
pub fn find_split(blocks: &[AudioBlock]) -> Option<(usize, &SplitBlock)> {
    blocks
        .iter()
        .enumerate()
        .find_map(|(position, block)| match &block.kind {
            AudioBlockKind::Split(split) => Some((position, split)),
            _ => None,
        })
}

/// The chain-level rules: at most one split, and nothing but the chain's own
/// `Input`/`Output` ports after a Y split.
pub fn validate_split_layout(blocks: &[AudioBlock]) -> Result<(), String> {
    let splits = blocks
        .iter()
        .filter(|block| matches!(block.kind, AudioBlockKind::Split(_)))
        .count();
    if splits > 1 {
        return Err(format!("a chain holds at most one split, found {splits}"));
    }
    let Some((position, split)) = find_split(blocks) else {
        return Ok(());
    };
    if split.end == SplitEnd::Y {
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
    }
    Ok(())
}
