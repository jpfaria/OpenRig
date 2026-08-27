//! Responsibility: describes one row of the block catalog.

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct BlockTypeCatalogEntry {
    pub effect_type: &'static str,
    pub display_label: &'static str,
    pub icon_kind: &'static str,
    pub use_panel_editor: bool,
}

#[derive(Debug, Clone)]
pub struct BlockModelCatalogEntry {
    pub effect_type: String,
    pub model_id: String,
    pub display_name: String,
    pub brand: String,
    pub type_label: String,
    pub supported_instruments: Vec<String>,
    pub knob_layout: &'static [block_core::KnobLayoutEntry],
}
