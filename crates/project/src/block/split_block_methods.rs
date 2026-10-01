//! Responsibility: states the structural rules a chain's splits obey.
//!
//! #328 (spec §11.1): a chain holds any number of splits, each with any number
//! of paths (at least two), nested to any depth. The rules are local to the
//! list a split sits in: a Y ends its own list (only the chain's own
//! `Input`/`Output` ports may follow one at the top level), and a path holds no
//! port or insert (an insert is a hardware loop the stream shares).

use super::split_block::{SplitBlock, SplitEnd, MIN_SPLIT_PATHS};
use super::split_lookup::splits;
use super::types::{AudioBlock, AudioBlockKind};

impl SplitBlock {
    /// The path rules, applied to every path at every depth.
    pub fn validate_structure(&self) -> Result<(), String> {
        if self.paths.len() < MIN_SPLIT_PATHS {
            return Err(format!(
                "split has {} path(s); a split needs at least {MIN_SPLIT_PATHS} paths",
                self.paths.len()
            ));
        }
        for path in &self.paths {
            for block in path {
                if matches!(
                    block.kind,
                    AudioBlockKind::Input(_)
                        | AudioBlockKind::Output(_)
                        | AudioBlockKind::Insert(_)
                ) {
                    return Err(format!(
                        "split path block '{}' is a {} block; a path holds no port or insert",
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
