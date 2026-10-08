//! Responsibility: brings a block's shown stream data up to its latest readings
//!
//! The stream timers call this every tick. The readings move inside the model
//! already shown when the row count holds; a new `BlockStreamData` comes back
//! only when what the panel shows changes shape (#1099).

use std::rc::Rc;

use slint::{Model, ModelRc, SharedString, VecModel};

use crate::{BlockStreamData, BlockStreamEntry};

/// The data to set after `entries`, or `None` when `current` already shows it.
pub(crate) fn sync_block_stream(
    current: &BlockStreamData,
    kind: SharedString,
    entries: &[block_core::StreamEntry],
) -> Option<BlockStreamData> {
    let active = !entries.is_empty();
    let same_shape = current.active == active && current.stream_kind == kind;
    if !active {
        return (!same_shape).then(|| BlockStreamData {
            active: false,
            stream_kind: kind,
            entries: ModelRc::default(),
        });
    }
    let rows = entries.iter().map(row);
    let shown = current
        .entries
        .as_any()
        .downcast_ref::<VecModel<BlockStreamEntry>>()
        .filter(|vm| same_shape && vm.row_count() == entries.len());
    match shown {
        Some(vm) => {
            for (i, r) in rows.enumerate() {
                if vm.row_data(i).as_ref() != Some(&r) {
                    vm.set_row_data(i, r);
                }
            }
            None
        }
        None => Some(BlockStreamData {
            active: true,
            stream_kind: kind,
            entries: ModelRc::from(Rc::new(VecModel::from_iter(rows))),
        }),
    }
}

fn row(e: &block_core::StreamEntry) -> BlockStreamEntry {
    BlockStreamEntry {
        key: e.key.as_str().into(),
        value: e.value,
        text: e.text.as_str().into(),
        peak: e.peak,
    }
}

#[cfg(test)]
#[path = "block_stream_sync_tests.rs"]
mod tests;
