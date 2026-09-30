//! Responsibility: says whether a chain's block list obeys the split rules.
//!
//! #328 (spec §1.1): the rules themselves live in Part 1's
//! `project::block::split_block_methods` (`validate_split_layout` for the
//! chain level, `SplitBlock::validate_structure` for the paths of EVERY split,
//! the Mix and the Y). Every command that reshapes a chain's blocks checks its
//! RESULT here before committing it, so no door — MCP, a MIDI map, the GUI —
//! can leave a project that fails to reopen, and the command layer never keeps
//! a second copy of the rules.

use anyhow::{anyhow, Result};
use project::block::{splits, validate_split_layout, AudioBlock};

pub fn ensure_split_rules(blocks: &[AudioBlock]) -> Result<()> {
    validate_split_layout(blocks).map_err(|e| anyhow!(e))?;
    for (_, split) in splits(blocks) {
        split.validate_structure().map_err(|e| anyhow!(e))?;
    }
    Ok(())
}
