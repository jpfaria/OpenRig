//! Responsibility: refuses an in-place edit whose `Select` points at an option it does not hold.
//!
//! #998: the swap takes the live nodes out of the pipeline and hands them to
//! the builder; a `Select` that fails there dropped them with it, leaving the
//! chain with no blocks (the dry input) until the next edit that built.

use anyhow::{anyhow, Result};

use project::block::AudioBlockKind;
use project::chain::Chain;

use crate::runtime_segments::ChainSegment;

/// Fails on the first enabled `Select` in `segments` whose selected option is
/// not among its options.
pub(crate) fn check_selects_build(chain: &Chain, segments: &[ChainSegment]) -> Result<()> {
    let blocks = segments
        .iter()
        .flat_map(|segment| segment.block_indices.iter())
        .filter_map(|&b| chain.blocks.get(b))
        .filter(|block| block.enabled);
    for block in blocks {
        if let AudioBlockKind::Select(select) = &block.kind {
            if select.selected_option().is_none() {
                return Err(anyhow!(
                    "chain '{}' select block '{}' references unknown option",
                    chain.id.0,
                    block.id.0
                ));
            }
        }
    }
    Ok(())
}
