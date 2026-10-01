//! Responsibility: states the structural rules a chain's splits obey.
//!
//! #328 (spec §10.1): a chain holds any number of splits, nested to any depth.
//! Two rules are left, and both are local to the list a split sits in: a Y ends
//! its own list (only the chain's own `Input`/`Output` ports may follow one at
//! the top level), and a path holds no port, insert or select.

use super::split_block::{SplitBlock, SplitEnd};
use super::split_lookup::splits;
use super::types::{AudioBlock, AudioBlockKind};

impl SplitBlock {
    /// The path rules, applied to both paths at every depth.
    pub fn validate_structure(&self) -> Result<(), String> {
        for path in [&self.a, &self.b] {
            for block in path {
                if matches!(
                    block.kind,
                    AudioBlockKind::Select(_)
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
            validate_list(path, false)?;
        }
        Ok(())
    }
}

/// The rules of a chain's own block list, and of every list below it.
pub fn validate_split_layout(blocks: &[AudioBlock]) -> Result<(), String> {
    validate_list(blocks, true)
}

/// `top_level`: the chain's own `Input`/`Output` ports may follow a Y there.
fn validate_list(blocks: &[AudioBlock], top_level: bool) -> Result<(), String> {
    for (position, split) in splits(blocks) {
        if split.end == SplitEnd::Y {
            let follower = blocks[position + 1..].iter().find(|block| {
                !(top_level
                    && matches!(
                        block.kind,
                        AudioBlockKind::Input(_) | AudioBlockKind::Output(_)
                    ))
            });
            if let Some(block) = follower {
                return Err(format!(
                    "a Y split ends the list it sits in, but block '{}' follows it",
                    block.id.0
                ));
            }
        }
        split.validate_structure()?;
    }
    Ok(())
}
