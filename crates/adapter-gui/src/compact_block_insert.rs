//! Responsibility: starts the insert flow for the compact view slot above a row.
//!
//! #328: a compact slot sits between two rows, and a row may live inside a
//! split's path — the new block joins the list of the row above the slot, not
//! `chain.blocks` at the row index.

use std::cell::RefCell;
use std::rc::Rc;

use slint::Global;

use crate::chain_block_lists::side_index;
use crate::compact_row_address::insert_slot;
use crate::state::ProjectSession;
use crate::{AppWindow, ChainGraphBridge};

pub(crate) fn start_insert_above_row(
    main_win: &AppWindow,
    session: &Rc<RefCell<Option<ProjectSession>>>,
    chain_index: i32,
    before_row: i32,
) {
    let slot = {
        let borrow = session.borrow();
        let Some(session) = borrow.as_ref() else {
            return;
        };
        let project = session.project.borrow();
        let Some(chain) = project.chains.get(chain_index.max(0) as usize) else {
            return;
        };
        insert_slot(chain, before_row.max(0) as usize)
    };
    match slot.path {
        None => main_win.invoke_start_block_insert(chain_index, slot.index as i32),
        Some(path) => ChainGraphBridge::get(main_win).invoke_start_path_insert(
            chain_index,
            path.split.0.as_str().into(),
            side_index(&path.side),
            slot.index as i32,
        ),
    }
}
