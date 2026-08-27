//! Responsibility: builds the picker lists a block type or model is chosen from.

use crate::chain_endpoint_labels::real_block_index_to_ui;
use crate::state::SelectedBlock;
use crate::AppWindow;
use crate::{BlockModelPickerItem, BlockTypePickerItem};
use project::catalog::{supported_block_models, supported_block_type, supported_block_types};
use project::chain::Chain;
use slint::{Model, SharedString, VecModel};

pub fn block_type_picker_items(instrument: &str) -> Vec<BlockTypePickerItem> {
    let mut seen = std::collections::BTreeSet::new();
    let mut items: Vec<BlockTypePickerItem> = supported_block_types()
        .into_iter()
        .filter(|item| seen.insert(item.effect_type))
        .map(|item| BlockTypePickerItem {
            effect_type: item.effect_type.into(),
            label: item.display_label.into(),
            subtitle: "".into(),
            icon_kind: item.icon_kind.into(),
            use_panel_editor: item.use_panel_editor,
            uses_model_catalog: block_core::effect_type_uses_model_catalog(item.effect_type),
            accent_color: crate::ui_state::accent_color_for_icon_kind(item.icon_kind),
            icon_source: slint::Image::default(),
        })
        .filter(|item| {
            instrument == block_core::INST_GENERIC
                || !block_model_picker_items(item.effect_type.as_str(), instrument).is_empty()
        })
        .collect();
    // Add I/O block types
    items.push(BlockTypePickerItem {
        effect_type: "input".into(),
        label: "INPUT".into(),
        subtitle: "".into(),
        icon_kind: "input".into(),
        use_panel_editor: false,
        uses_model_catalog: false,
        accent_color: crate::ui_state::accent_color_for_icon_kind("routing"),
        icon_source: slint::Image::default(),
    });
    items.push(BlockTypePickerItem {
        effect_type: "output".into(),
        label: "OUTPUT".into(),
        subtitle: "".into(),
        icon_kind: "output".into(),
        use_panel_editor: false,
        uses_model_catalog: false,
        accent_color: crate::ui_state::accent_color_for_icon_kind("routing"),
        icon_source: slint::Image::default(),
    });
    items.push(BlockTypePickerItem {
        effect_type: "insert".into(),
        label: "INSERT".into(),
        subtitle: "".into(),
        icon_kind: "insert".into(),
        use_panel_editor: false,
        uses_model_catalog: false,
        accent_color: crate::ui_state::accent_color_for_icon_kind("insert"),
        icon_source: slint::Image::default(),
    });
    items
}

pub(crate) fn block_model_picker_items(
    effect_type: &str,
    instrument: &str,
) -> Vec<BlockModelPickerItem> {
    let all_models = supported_block_models(effect_type).unwrap_or_default();
    log::trace!(
        "[block_model_picker_items] effect_type='{}', instrument='{}', total_models={}",
        effect_type,
        instrument,
        all_models.len()
    );
    all_models
        .into_iter()
        .filter(|item| {
            instrument == block_core::INST_GENERIC
                || item.supported_instruments.iter().any(|i| i == instrument)
        })
        .map(|item| {
            let brand = &item.brand;
            let label = if brand.is_empty() || brand == block_core::BRAND_NATIVE {
                item.display_name.clone()
            } else {
                let brand_display = block_core::capitalize_first(brand);
                format!("{} {}", brand_display, item.display_name)
            };
            let visual = project::catalog::resolve_color_scheme(
                &item.effect_type,
                &item.brand,
                &item.model_id,
            );
            let [r, g, b] = visual.panel_bg;
            let panel_bg = slint::Color::from_argb_u8(0xff, r, g, b);
            let [r, g, b] = visual.panel_text;
            let panel_text = slint::Color::from_argb_u8(0xff, r, g, b);
            let [r, g, b] = visual.brand_strip_bg;
            let brand_strip_bg = slint::Color::from_argb_u8(0xff, r, g, b);
            BlockModelPickerItem {
                effect_type: item.effect_type.clone().into(),
                model_id: item.model_id.clone().into(),
                label: label.into(),
                display_name: item.display_name.clone().into(),
                subtitle: "".into(),
                icon_kind: supported_block_type(effect_type)
                    .map(|entry| entry.icon_kind)
                    .unwrap_or(effect_type)
                    .into(),
                brand: item.brand.clone().into(),
                type_label: item.type_label.clone().into(),
                panel_bg,
                panel_text,
                brand_strip_bg,
                model_font: visual.model_font.into(),
                available: project::catalog::is_model_available(&item.effect_type, &item.model_id),
                thumbnail_path: "".into(),
            }
        })
        .collect()
}

pub(crate) fn block_model_picker_labels(items: &[BlockModelPickerItem]) -> Vec<SharedString> {
    items.iter().map(|item| item.label.clone()).collect()
}

pub(crate) fn set_selected_block(
    window: &AppWindow,
    selected_block: Option<&SelectedBlock>,
    chain: Option<&Chain>,
) {
    if let Some(selected_block) = selected_block {
        let ui_index = chain
            .and_then(|c| real_block_index_to_ui(c, selected_block.block_index))
            .map(|i| i as i32)
            .unwrap_or(selected_block.block_index as i32);
        window.set_selected_chain_block_chain_index(selected_block.chain_index as i32);
        window.set_selected_chain_block_index(ui_index);
    } else {
        window.set_selected_chain_block_chain_index(-1);
        window.set_selected_chain_block_index(-1);
    }
}

pub(crate) fn block_type_index(effect_type: &str, instrument: &str) -> i32 {
    block_type_picker_items(instrument)
        .into_iter()
        .position(|item| item.effect_type.as_str() == effect_type)
        .map(|index| index as i32)
        .unwrap_or(-1)
}

pub(crate) fn block_model_index_from_items(
    items: &VecModel<BlockModelPickerItem>,
    model_id: &str,
) -> i32 {
    for i in 0..items.row_count() {
        if let Some(item) = items.row_data(i) {
            if item.model_id.as_str() == model_id {
                return i as i32;
            }
        }
    }
    0
}

pub(crate) fn block_model_index(effect_type: &str, model_id: &str, instrument: &str) -> i32 {
    supported_block_models(effect_type)
        .unwrap_or_default()
        .into_iter()
        .filter(|item| {
            instrument == block_core::INST_GENERIC
                || item.supported_instruments.iter().any(|i| i == instrument)
        })
        .position(|item| item.model_id == model_id)
        .map(|index| index as i32)
        .unwrap_or(-1)
}
