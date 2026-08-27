//! Responsibility: builds what the chains screen shows.

pub use crate::block_picker_items::block_type_picker_items;
pub(crate) use crate::block_picker_items::{
    block_model_index, block_model_index_from_items, block_model_picker_items,
    block_model_picker_labels, block_type_index, set_selected_block,
};
pub(crate) use crate::chain_endpoint_labels::{format_channel_list, real_block_index_to_ui};
pub(crate) use crate::project_chains_refresh::replace_project_chains;
pub(crate) use crate::project_view_assets::load_screenshot_image;

// The crate-root test modules reach these through `project_view::`, where
// they were defined before the split (#873).
#[cfg(test)]
pub(crate) use crate::chain_block_item::chain_block_item_from_block;
#[cfg(test)]
pub(crate) use crate::chain_endpoint_labels::chain_endpoint_label;
#[cfg(test)]
pub(crate) use crate::project_view_tooltips::{chain_inputs_tooltip, chain_outputs_tooltip};
