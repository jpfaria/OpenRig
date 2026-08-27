//! Responsibility: keeps the historical `ui_state` path pointing at the five things it held.
//!
//! It was responsible for the block drawer's state, a block kind's icon, the
//! I/O binding models, where an insert slot goes, and the chain I/O labels
//! (#873).

pub use crate::block_drawer_state::block_drawer_state;
pub use crate::block_icon::{accent_color_for_icon_kind, block_family_for_kind};
pub use crate::chain_io_labels::chain_routing_summary;
pub(crate) use crate::chain_io_labels::{chain_io_chip_label_from_bindings, channels_label};

// `ui_state_tests.rs` hangs off this module and reaches these through
// `super::*`, exactly where they were defined before the split (#873).
#[cfg(test)]
pub use crate::block_drawer_state::BlockDrawerMode;
#[cfg(test)]
pub use crate::block_icon::icon_index_for_icon_kind;
#[cfg(test)]
pub use crate::chain_io_labels::chain_io_chip_label;
#[cfg(test)]
pub use crate::insertion_slots::insertion_slot_indices;
#[cfg(test)]
pub use crate::io_binding_models::{resolve_block_io_endpoint, ui_bindings};

#[cfg(test)]
#[path = "ui_state_tests.rs"]
mod tests;
