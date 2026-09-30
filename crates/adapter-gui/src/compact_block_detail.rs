//! Responsibility: opens the editor of the block shown on a compact view row.
//!
//! #328: a row may be the split itself or a block inside one of its paths, so
//! the row index is resolved to the same editor a click on its graph node opens.

use std::cell::RefCell;
use std::rc::Rc;

use slint::Global;

use project::block::AudioBlockKind;

use crate::chain_block_lists::side_index;
use crate::compact_row_address::{compact_rows, RowAddress};
use crate::state::ProjectSession;
use crate::{AppWindow, ChainGraphBridge, ChainGraphOverlayState};

/// The split editor's kind index for the split's own parameters.
const SPLIT_EDITOR_KIND: i32 = 0;

pub(crate) fn open_row_detail(
    main_win: &AppWindow,
    session: &Rc<RefCell<Option<ProjectSession>>>,
    chain_index: i32,
    row: i32,
) {
    let target = {
        let borrow = session.borrow();
        let Some(session) = borrow.as_ref() else {
            return;
        };
        let project = session.project.borrow();
        let Some(chain) = project.chains.get(chain_index.max(0) as usize) else {
            return;
        };
        compact_rows(chain)
            .into_iter()
            .nth(row.max(0) as usize)
            .map(|(address, block)| (address, matches!(block.kind, AudioBlockKind::Split(_))))
    };
    match target {
        Some((_, true)) => ChainGraphOverlayState::get(main_win)
            .invoke_open_split_editor(chain_index, SPLIT_EDITOR_KIND),
        Some((RowAddress { path: None, index }, false)) => {
            main_win.invoke_select_chain_block(chain_index, index as i32)
        }
        Some((
            RowAddress {
                path: Some(path),
                index,
            },
            false,
        )) => ChainGraphBridge::get(main_win).invoke_open_path_block(
            chain_index,
            path.split.0.as_str().into(),
            side_index(&path.side),
            index as i32,
        ),
        None => {}
    }
}
