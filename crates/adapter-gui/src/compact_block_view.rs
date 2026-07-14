//! #787 — projection of a chain's blocks into the compact view's row model.
//!
//! Split out of `project_view.rs` (line cap) when the compact row gained its own
//! geometry: the parameter strip wraps into lines, the row grows to fit them,
//! and a block with 2+ parameter groups gets a tab bar. The maths lives in
//! `compact_block_layout`; the active tab in `compact_block_tabs`.

use std::rc::Rc;

use slint::{ModelRc, SharedString, VecModel};

use project::catalog::supported_block_type;
use project::project::Project;

use crate::block_editor::{
    block_editor_data, block_parameter_items_for_editor, build_knob_overlays, parameter_groups,
};
use crate::block_editor_param_items::DEFAULT_PARAM_GROUP;
use crate::block_editor_param_tabs::retag_for_group;
use crate::compact_block_layout::{
    assign_overlay_lines, assign_strip_lines, row_height_px, row_y_offsets,
};
use crate::compact_block_tabs::active_group_index;
use crate::eq::{build_curve_editor_points, build_multi_slider_points};
use crate::project_view::block_model_picker_items;
use crate::{
    BlockKnobOverlay, BlockParameterItem, CompactBlockItem, CompactOverlayLine, CompactParamLine,
    SELECT_SELECTED_BLOCK_ID,
};

/// Group the laid-out cells by their `strip_line` — Slint cannot filter inside a
/// `for`, so it renders one `HorizontalLayout` per line of this model.
fn param_lines(params: &[BlockParameterItem], lines: i32) -> Vec<CompactParamLine> {
    (0..lines)
        .map(|line| CompactParamLine {
            cells: ModelRc::from(Rc::new(VecModel::from(
                params
                    .iter()
                    .filter(|it| it.strip_line == line)
                    .cloned()
                    .collect::<Vec<_>>(),
            ))),
        })
        .collect()
}

fn overlay_lines(overlays: &[BlockKnobOverlay], lines: i32) -> Vec<CompactOverlayLine> {
    (0..lines)
        .map(|line| CompactOverlayLine {
            knobs: ModelRc::from(Rc::new(VecModel::from(
                overlays
                    .iter()
                    .filter(|k| k.strip_line == line)
                    .cloned()
                    .collect::<Vec<_>>(),
            ))),
        })
        .collect()
}

/// Tab labels of a block's parameters, in first-appearance order. The synthetic
/// model-picker row is pinned to every tab, so it is never a group of its own
/// (same rule as the detached editor, #780).
fn compact_parameter_groups(params: &[BlockParameterItem]) -> Vec<String> {
    let groupable: Vec<BlockParameterItem> = params
        .iter()
        .filter(|it| it.path.as_str() != SELECT_SELECTED_BLOCK_ID)
        .cloned()
        .collect();
    parameter_groups(&groupable)
}

pub(crate) fn build_compact_blocks(project: &Project, chain_index: usize) -> Vec<CompactBlockItem> {
    let Some(chain) = project.chains.get(chain_index) else {
        return Vec::new();
    };
    let mut items: Vec<CompactBlockItem> = chain
        .blocks
        .iter()
        .enumerate()
        .filter_map(|(block_index, block)| {
            let editor_data = block_editor_data(block)?;
            let effect_type = editor_data.effect_type.clone();
            let model_id = editor_data.model_id.clone();

            // Parameters: tag each row with the tab it belongs to (#780 keeps the
            // model FULL so no tab's values are ever dropped), then wrap the
            // active tab's rows into strip lines (#787).
            let mut params = block_parameter_items_for_editor(&editor_data);
            let groups = compact_parameter_groups(&params);
            let active_index = active_group_index(&block.id.0, &groups);
            let active = groups
                .get(active_index)
                .map(String::as_str)
                .unwrap_or(DEFAULT_PARAM_GROUP);
            params = retag_for_group(&params, active);
            let strip_lines = assign_strip_lines(&mut params);

            let knob_layout = project::catalog::model_knob_layout(&effect_type, &model_id);
            let mut overlays = build_knob_overlays(knob_layout, &params);
            let ms_pts = build_multi_slider_points(&effect_type, &model_id, &editor_data.params);
            let ce_pts = build_curve_editor_points(&effect_type, &model_id, &editor_data.params);

            // A model with a curated `knob_layout` renders its overlays instead of
            // the generic strip, and an EQ block renders its widget instead of
            // both — so the row's height follows whichever one it actually shows.
            let has_eq = !ms_pts.is_empty() || !ce_pts.is_empty();
            let lines = if has_eq {
                1
            } else if !overlays.is_empty() {
                assign_overlay_lines(&mut overlays)
            } else {
                strip_lines
            };
            let has_tabs = !has_eq && overlays.is_empty() && groups.len() > 1;

            let icon_kind = supported_block_type(&effect_type)
                .map(|t| t.icon_kind.to_string())
                .unwrap_or_default();
            let visual = project::catalog::supported_block_models(&effect_type)
                .ok()
                .and_then(|models| models.into_iter().find(|m| m.model_id == model_id));

            let cell_lines = param_lines(&params, lines);
            let knob_lines = overlay_lines(&overlays, lines);

            Some(CompactBlockItem {
                chain_index: chain_index as i32,
                block_index: block_index as i32,
                block_id: block.id.0.clone().into(),
                effect_type: effect_type.clone().into(),
                model_id: model_id.clone().into(),
                icon_kind: icon_kind.clone().into(),
                brand: visual
                    .as_ref()
                    .map(|v| v.brand.clone())
                    .unwrap_or_default()
                    .into(),
                display_name: visual
                    .as_ref()
                    .map(|v| v.display_name.clone())
                    .unwrap_or_default()
                    .into(),
                type_label: visual
                    .as_ref()
                    .map(|v| v.type_label.clone())
                    .unwrap_or_default()
                    .into(),
                enabled: block.enabled,
                panel_bg: {
                    let brand_str = visual.as_ref().map(|v| v.brand.as_str()).unwrap_or("");
                    let vc =
                        project::catalog::resolve_color_scheme(&effect_type, brand_str, &model_id);
                    let [r, g, b] = vc.panel_bg;
                    slint::Color::from_argb_u8(0xff, r, g, b)
                },
                panel_text: {
                    let brand_str = visual.as_ref().map(|v| v.brand.as_str()).unwrap_or("");
                    let vc =
                        project::catalog::resolve_color_scheme(&effect_type, brand_str, &model_id);
                    let [r, g, b] = vc.panel_text;
                    slint::Color::from_argb_u8(0xff, r, g, b)
                },
                accent_color: crate::ui_state::accent_color_for_icon_kind(&icon_kind),
                display_label: {
                    let bt = supported_block_type(&effect_type);
                    bt.map(|e| e.display_label).unwrap_or("BLOCK").into()
                },
                icon_source: slint::Image::default(),
                knob_overlays: ModelRc::from(Rc::new(VecModel::from(overlays))),
                parameter_items: ModelRc::from(Rc::new(VecModel::from(params))),
                multi_slider_points: ModelRc::from(Rc::new(VecModel::from(ms_pts))),
                curve_editor_points: ModelRc::from(Rc::new(VecModel::from(ce_pts))),
                model_labels: {
                    let instrument = chain.instrument.as_str();
                    let items = block_model_picker_items(&effect_type, instrument);
                    let labels: Vec<SharedString> = items.iter().map(|i| i.label.clone()).collect();
                    ModelRc::from(Rc::new(VecModel::from(labels)))
                },
                model_selected_index: {
                    let instrument = chain.instrument.as_str();
                    let items = block_model_picker_items(&effect_type, instrument);
                    items
                        .iter()
                        .position(|i| i.model_id.as_str() == model_id)
                        .map(|i| i as i32)
                        .unwrap_or(-1)
                },
                models: {
                    let instrument = chain.instrument.as_str();
                    let items = block_model_picker_items(&effect_type, instrument);
                    ModelRc::from(Rc::new(VecModel::from(items)))
                },
                filtered_models: {
                    let instrument = chain.instrument.as_str();
                    let items = block_model_picker_items(&effect_type, instrument);
                    ModelRc::from(Rc::new(VecModel::from(items)))
                },
                stream_data: Default::default(),
                has_external_gui: project::catalog::block_has_external_gui(&effect_type),
                parameter_groups: ModelRc::from(Rc::new(VecModel::from(
                    groups
                        .iter()
                        .map(|g| SharedString::from(g.as_str()))
                        .collect::<Vec<_>>(),
                ))),
                active_parameter_group: active_index as i32,
                parameter_lines: ModelRc::from(Rc::new(VecModel::from(cell_lines))),
                overlay_lines: ModelRc::from(Rc::new(VecModel::from(knob_lines))),
                row_height: row_height_px(lines, has_tabs),
                row_y: 0.0,
            })
        })
        .collect();

    let heights: Vec<f32> = items.iter().map(|it| it.row_height).collect();
    for (it, y) in items.iter_mut().zip(row_y_offsets(&heights)) {
        it.row_y = y;
    }
    items
}
