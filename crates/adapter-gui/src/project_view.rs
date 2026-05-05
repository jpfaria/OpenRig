use crate::block_editor::{
    block_editor_data, block_parameter_items_for_editor, block_parameter_items_for_model,
    build_knob_overlays,
};
use crate::eq::{build_curve_editor_points, build_multi_slider_points};
use crate::state::SelectedBlock;
use crate::ui_state::chain_routing_summary;
use crate::AppWindow;
use crate::{BlockModelPickerItem, BlockTypePickerItem, CompactBlockItem, ProjectChainItem};
use infra_cpal::AudioDeviceDescriptor;
use project::block::AudioBlockKind;
use project::catalog::{supported_block_models, supported_block_type, supported_block_types};
use project::chain::Chain;
use project::project::Project;
use slint::{Model, ModelRc, SharedString, VecModel};
use std::rc::Rc;

pub(crate) use crate::project_view_assets::{load_screenshot_image, load_thumbnail_image};
pub(crate) use crate::project_view_tooltips::{chain_inputs_tooltip, chain_outputs_tooltip};

pub(crate) fn block_type_picker_items(instrument: &str) -> Vec<BlockTypePickerItem> {
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
        accent_color: crate::ui_state::accent_color_for_icon_kind("routing"),
        icon_source: slint::Image::default(),
    });
    items.push(BlockTypePickerItem {
        effect_type: "output".into(),
        label: "OUTPUT".into(),
        subtitle: "".into(),
        icon_kind: "output".into(),
        use_panel_editor: false,
        accent_color: crate::ui_state::accent_color_for_icon_kind("routing"),
        icon_source: slint::Image::default(),
    });
    items.push(BlockTypePickerItem {
        effect_type: "insert".into(),
        label: "INSERT".into(),
        subtitle: "".into(),
        icon_kind: "insert".into(),
        use_panel_editor: false,
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
                photo_offset_x: visual.photo_offset_x,
                photo_offset_y: visual.photo_offset_y,
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

pub(crate) fn build_compact_blocks(project: &Project, chain_index: usize) -> Vec<CompactBlockItem> {
    let Some(chain) = project.chains.get(chain_index) else {
        return Vec::new();
    };
    chain
        .blocks
        .iter()
        .enumerate()
        .filter_map(|(block_index, block)| {
            let editor_data = block_editor_data(block)?;
            let effect_type = editor_data.effect_type.clone();
            let model_id = editor_data.model_id.clone();
            let params = block_parameter_items_for_editor(&editor_data);
            let knob_layout = project::catalog::model_knob_layout(&effect_type, &model_id);
            let overlays = build_knob_overlays(knob_layout, &params);
            let ms_pts = build_multi_slider_points(&effect_type, &model_id, &editor_data.params);
            let ce_pts = build_curve_editor_points(&effect_type, &model_id, &editor_data.params);
            let icon_kind = supported_block_type(&effect_type)
                .map(|t| t.icon_kind.to_string())
                .unwrap_or_default();
            let visual = project::catalog::supported_block_models(&effect_type)
                .ok()
                .and_then(|models| models.into_iter().find(|m| m.model_id == model_id));

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
            })
        })
        .collect()
}

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

pub(crate) fn replace_project_chains(
    model: &Rc<VecModel<ProjectChainItem>>,
    project: &Project,
    input_devices: &[AudioDeviceDescriptor],
    output_devices: &[AudioDeviceDescriptor],
) {
    let items = project
        .chains
        .iter()
        .enumerate()
        .map(|(index, chain)| {
            // Latency starts at 0 (badge hidden). The sonar probe populates
            // `latency_ms` with a measured value for up to 10 s when the
            // user clicks the probe button on the chain card.
            let latency_ms = 0.0_f32;
            ProjectChainItem {
                instrument: chain.instrument.clone().into(),
                title: chain
                    .description
                    .clone()
                    .unwrap_or_else(|| {
                        rust_i18n::t!("default-chain-name", n = index + 1).to_string()
                    })
                    .into(),
                subtitle: chain_routing_summary(chain).into(),
                enabled: chain.enabled,
                block_count_label: {
                    let effect_block_count = chain
                        .blocks
                        .iter()
                        .filter(|b| {
                            !matches!(
                                &b.kind,
                                AudioBlockKind::Input(_) | AudioBlockKind::Output(_)
                            )
                        })
                        .count();
                    if effect_block_count == 1 {
                        "1 block".into()
                    } else {
                        format!("{} blocks", effect_block_count).into()
                    }
                },
                input_label: {
                    let input_chs: Vec<usize> = chain
                        .input_blocks()
                        .into_iter()
                        .flat_map(|(_, ib)| {
                            ib.entries.iter().flat_map(|e| e.channels.iter().copied())
                        })
                        .collect();
                    chain_endpoint_label("In", &input_chs).into()
                },
                input_tooltip: chain_inputs_tooltip(chain, project, input_devices).into(),
                output_label: {
                    let output_chs: Vec<usize> = chain
                        .output_blocks()
                        .into_iter()
                        .flat_map(|(_, ob)| {
                            ob.entries.iter().flat_map(|e| e.channels.iter().copied())
                        })
                        .collect();
                    chain_endpoint_label("Out", &output_chs).into()
                },
                output_tooltip: chain_outputs_tooltip(chain, project, output_devices).into(),
                latency_ms,
                blocks: {
                    let first_input_idx = chain
                        .blocks
                        .iter()
                        .position(|b| matches!(&b.kind, AudioBlockKind::Input(_)));
                    let last_output_idx = chain
                        .blocks
                        .iter()
                        .rposition(|b| matches!(&b.kind, AudioBlockKind::Output(_)));
                    log::info!(
                        "[replace_project_chains] chain[{}] '{}' UI blocks:",
                        index,
                        chain.description.as_deref().unwrap_or("")
                    );
                    for (real_idx, b) in chain.blocks.iter().enumerate() {
                        if Some(real_idx) == first_input_idx || Some(real_idx) == last_output_idx {
                            continue;
                        }
                        log::info!(
                            "[replace_project_chains]   real_index={} kind={}",
                            real_idx,
                            b.model_ref()
                                .map(|m| format!("{}/{}", m.effect_type, m.model))
                                .unwrap_or_else(|| "io/insert".to_string())
                        );
                    }
                    ModelRc::from(Rc::new(VecModel::from(
                        chain
                            .blocks
                            .iter()
                            .enumerate()
                            .filter(|(i, _)| {
                                // Hide only the first Input (fixed chip) and last Output (fixed chip)
                                Some(*i) != first_input_idx && Some(*i) != last_output_idx
                            })
                            .map(|(real_idx, b)| {
                                let mut item = chain_block_item_from_block(b);
                                item.real_index = real_idx as i32;
                                item
                            })
                            .collect::<Vec<_>>(),
                    )))
                },
            }
        })
        .collect::<Vec<_>>();
    model.set_vec(items);
}

pub(crate) fn chain_endpoint_label(prefix: &str, _channels: &[usize]) -> String {
    prefix.to_string()
}

pub(crate) fn format_channel_list(channels: &[usize]) -> String {
    if channels.is_empty() {
        "-".to_string()
    } else {
        channels
            .iter()
            .map(|channel| (channel + 1).to_string())
            .collect::<Vec<_>>()
            .join(", ")
    }
}

/// Map a real chain.blocks index to the UI block index (which excludes hidden first Input and last Output).
pub(crate) fn real_block_index_to_ui(chain: &Chain, real_index: usize) -> Option<usize> {
    let first_input_idx = chain
        .blocks
        .iter()
        .position(|b| matches!(&b.kind, AudioBlockKind::Input(_)));
    let last_output_idx = chain
        .blocks
        .iter()
        .rposition(|b| matches!(&b.kind, AudioBlockKind::Output(_)));
    let mut visible_count = 0;
    for (idx, _) in chain.blocks.iter().enumerate() {
        if Some(idx) == first_input_idx || Some(idx) == last_output_idx {
            continue;
        }
        if idx == real_index {
            return Some(visible_count);
        }
        visible_count += 1;
    }
    None
}
