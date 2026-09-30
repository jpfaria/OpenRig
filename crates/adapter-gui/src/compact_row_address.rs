//! Responsibility: maps a compact view row index to the block it shows.
//!
//! #328: the compact view lists every block of the chain in signal order, the
//! blocks inside a split's paths included (the split, then path A, then path
//! B). A row index is therefore not a position in `chain.blocks`; this is the
//! one place that turns it into the list and position the block lives at.

use domain::ids::BlockId;
use project::block::{AudioBlock, AudioBlockKind, PathRef, PathSide};
use project::chain::Chain;

/// Where a block sits: the list (`None` = the chain itself) and its position.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct RowAddress {
    pub(crate) path: Option<PathRef>,
    pub(crate) index: usize,
}

/// Every block of the chain in compact row order, with where it sits.
pub(crate) fn compact_rows(chain: &Chain) -> Vec<(RowAddress, &AudioBlock)> {
    let mut rows = Vec::new();
    push_rows(&chain.blocks, None, &mut rows);
    rows
}

fn push_rows<'a>(
    blocks: &'a [AudioBlock],
    path: Option<PathRef>,
    rows: &mut Vec<(RowAddress, &'a AudioBlock)>,
) {
    for (index, block) in blocks.iter().enumerate() {
        rows.push((
            RowAddress {
                path: path.clone(),
                index,
            },
            block,
        ));
        if let AudioBlockKind::Split(split) = &block.kind {
            for (side, lane) in [(PathSide::A, &split.a), (PathSide::B, &split.b)] {
                let lane_path = PathRef {
                    split: block.id.clone(),
                    side,
                };
                push_rows(lane, Some(lane_path), rows);
            }
        }
    }
}

/// The block shown on compact row `row`.
pub(crate) fn row_block(chain: &Chain, row: usize) -> Option<&AudioBlock> {
    compact_rows(chain).get(row).map(|(_, block)| *block)
}

/// The list and position of the block shown on compact row `row`.
pub(crate) fn row_address(chain: &Chain, row: usize) -> Option<RowAddress> {
    compact_rows(chain)
        .into_iter()
        .nth(row)
        .map(|(address, _)| address)
}

/// Where a block inserted at the slot above row `before_row` goes: right after
/// the row above it, in that row's list. Under a split row the slot opens the
/// head of path A, which is the row drawn right below it.
pub(crate) fn insert_slot(chain: &Chain, before_row: usize) -> RowAddress {
    let rows = compact_rows(chain);
    let Some((above, block)) = before_row.checked_sub(1).and_then(|r| rows.get(r)) else {
        return RowAddress {
            path: None,
            index: 0,
        };
    };
    if matches!(block.kind, AudioBlockKind::Split(_)) {
        return RowAddress {
            path: Some(PathRef {
                split: block.id.clone(),
                side: PathSide::A,
            }),
            index: 0,
        };
    }
    RowAddress {
        path: above.path.clone(),
        index: above.index + 1,
    }
}

/// Where the block on row `from_row` lands when dropped on the slot above
/// `before_row`: its id and its new position in its own list. `None` when the
/// drop would not move it or would carry it into another list — a drag only
/// reorders the list the block is in.
pub(crate) fn move_target(
    chain: &Chain,
    from_row: usize,
    before_row: usize,
) -> Option<(BlockId, RowAddress)> {
    let block = row_block(chain, from_row)?;
    let from = row_address(chain, from_row)?;
    let slot = insert_slot(chain, before_row);
    if slot.path != from.path {
        return None;
    }
    let index = if slot.index > from.index {
        slot.index - 1
    } else {
        slot.index
    };
    if index == from.index {
        return None;
    }
    Some((
        block.id.clone(),
        RowAddress {
            path: from.path,
            index,
        },
    ))
}
