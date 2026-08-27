//! Responsibility: projects one block into the tile the chain row shows.

use crate::block_editor::block_parameter_items_for_model;
use crate::project_view_assets::load_thumbnail_image;
use project::block::AudioBlockKind;
use project::catalog::supported_block_type;
use slint::{ModelRc, VecModel};
use std::rc::Rc;

pub(crate) fn chain_block_item_from_block(
    block: &project::block::AudioBlock,
) -> crate::ChainBlockItem {
    use crate::ui_state::block_family_for_kind;
    use crate::ChainBlockItem;
    let (kind, label) = match &block.kind {
        AudioBlockKind::Input(_) => ("input".to_string(), "input".to_string()),
        AudioBlockKind::Output(_) => ("output".to_string(), "output".to_string()),
        AudioBlockKind::Insert(_) => ("insert".to_string(), "insert".to_string()),
        AudioBlockKind::Select(select) => select
            .selected_option()
            .and_then(|option| option.model_ref())
            .map(|model| (model.effect_type.to_string(), model.model.to_string()))
            .unwrap_or_else(|| ("select".to_string(), "select".to_string())),
        _ => block
            .model_ref()
            .map(|b| (b.effect_type.to_string(), b.model.to_string()))
            .unwrap_or_else(|| ("core".to_string(), "block".to_string())),
    };
    let family = block_family_for_kind(&kind).to_string();
    let block_type = supported_block_type(&kind);
    let (thumbnail, has_thumbnail, thumb_width, thumb_height) = load_thumbnail_image(&kind, &label);

    // I/O and Insert blocks are not registered effect types, so resolve icon_kind/type_label directly
    let is_io = matches!(
        block.kind,
        AudioBlockKind::Input(_) | AudioBlockKind::Output(_) | AudioBlockKind::Insert(_)
    );
    let resolved_icon_kind: String = if is_io {
        kind.clone()
    } else {
        block_type
            .as_ref()
            .map(|e| e.icon_kind)
            .unwrap_or("core")
            .to_string()
    };
    let resolved_type_label: &str = if is_io {
        match &block.kind {
            AudioBlockKind::Input(_) => "INPUT",
            AudioBlockKind::Output(_) => "OUTPUT",
            AudioBlockKind::Insert(_) => "INSERT",
            _ => "BLOCK",
        }
    } else {
        block_type
            .as_ref()
            .map(|e| e.display_label)
            .unwrap_or("BLOCK")
    };

    let accent_color = crate::ui_state::accent_color_for_icon_kind(&resolved_icon_kind);

    // Hover-tooltip metadata. Empty for I/O and Insert blocks — there is no
    // model picker behind those, so the tooltip would show no useful
    // information. For everything else, the catalog delegates to the right
    // block-* crate per effect type to give us the display name, DSP
    // backend label (NATIVE/NAM/IR/LV2) and brand slug. The parameter
    // summary uses the same formatter as the editor so units, precision
    // and labels stay in sync without a parallel formatter. Issue #333.
    let (display_name, backend_label, brand, param_entries) = if is_io {
        (String::new(), String::new(), String::new(), Vec::new())
    } else {
        let name = project::catalog::model_display_name(&kind, &label).to_string();
        let backend = project::catalog::model_type_label(&kind, &label).to_uppercase();
        let brand = project::catalog::model_brand(&kind, &label).to_string();
        let entries = match block.model_ref() {
            Some(model_ref) => collect_block_param_entries(
                model_ref.effect_type,
                model_ref.model,
                model_ref.params,
            ),
            None => Vec::new(),
        };
        (name, backend, brand, entries)
    };

    ChainBlockItem {
        kind: kind.into(),
        icon_kind: resolved_icon_kind.into(),
        type_label: resolved_type_label.into(),
        label: label.into(),
        family: family.into(),
        enabled: block.enabled,
        unavailable: !project::project_disable_unavailable::block_model_is_available(&block.kind),
        real_index: 0,
        thumbnail,
        has_thumbnail,
        thumb_width,
        thumb_height,
        accent_color,
        icon_source: slint::Image::default(),
        display_name: display_name.into(),
        backend_label: backend_label.into(),
        brand: brand.into(),
        param_entries: ModelRc::from(Rc::new(VecModel::from(param_entries))),
    }
}

/// Collect the visible parameters of a block as `(label, value, unit)`
/// triples for the hover tooltip. Skips entries whose `value_text` is
/// empty (e.g. unset optional fields) so the list stays informative.
/// Reuses the editor's formatter so the tooltip values match the editor.
fn collect_block_param_entries(
    effect_type: &str,
    model_id: &str,
    params: &project::param::ParameterSet,
) -> Vec<crate::BlockParamSummaryEntry> {
    use crate::BlockParamSummaryEntry;
    block_parameter_items_for_model(effect_type, model_id, params)
        .into_iter()
        .filter(|item| !item.value_text.is_empty())
        .map(|item| BlockParamSummaryEntry {
            label: item.label.to_string().to_uppercase().into(),
            value: item.value_text,
            unit: item.unit_text,
        })
        .collect()
}
