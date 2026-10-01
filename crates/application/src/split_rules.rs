//! Responsibility: says whether a chain's block list obeys the split rules.
//!
//! #328 (spec §10.1): the rules themselves live in
//! `project::block::split_block_methods`, whose `validate_split_layout` walks
//! the chain's list and every path below it, at any depth. Every command that reshapes a chain's blocks checks its
//! RESULT here before committing it, so no door — MCP, a MIDI map, the GUI —
//! can leave a project that fails to reopen, and the command layer never keeps
//! a second copy of the rules.

use anyhow::{anyhow, Result};
use project::block::{validate_split_layout, AudioBlock};

pub fn ensure_split_rules(blocks: &[AudioBlock]) -> Result<()> {
    validate_split_layout(blocks).map_err(|e| anyhow!(e))
}
