//! Responsibility: builds the view model of the looper panel.
//! #323 — the pure view model of the looper panel: persisted parameters
//! (project) merged with the live transport state the audio thread publishes,
//! turned into the rows the panel renders.
//!
//! Pure and testable: no Slint window, no runtime handle. The GUI timer calls
//! it and hands the result to the model (the "screen has no business logic"
//! law).

pub(crate) use crate::looper_items::{any_looper_active, looper_items, looper_items_from_config};
pub(crate) use crate::looper_rows::{
    apply_looper_endpoints_to_rows, apply_looper_presets_to_rows, chain_preset_ids,
    write_chain_looper_row,
};
pub(crate) use crate::looper_vocabulary::clock_label;

#[cfg(test)]
#[path = "looper_view_tests.rs"]
mod tests;
