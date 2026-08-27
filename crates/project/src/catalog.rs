//! Responsibility: routes the block catalog's public surface.

pub use crate::catalog_availability::is_model_available;
pub use crate::catalog_build::build_block_kind;
pub use crate::catalog_colors::{model_color_override, resolve_color_scheme};
pub use crate::catalog_listing::{
    supported_block_models, supported_block_type, supported_block_types,
};
pub use crate::catalog_model_info::{
    block_has_external_gui, model_brand, model_display_name, model_knob_layout, model_stream_kind,
    model_type_label,
};
pub use crate::catalog_types::{BlockModelCatalogEntry, BlockTypeCatalogEntry};

#[cfg(test)]
#[path = "catalog_tests.rs"]
mod tests;
