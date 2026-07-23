use crate::block_editor::block_parameter_items_for_model;
use crate::state::SelectedBlock;
use crate::ui_state::{chain_io_chip_label_from_bindings, chain_routing_summary};
use crate::AppWindow;
use crate::{BlockModelPickerItem, BlockTypePickerItem, ProjectChainItem};
use infra_cpal::AudioDeviceDescriptor;
use infra_filesystem::IoBinding;
use project::block::AudioBlockKind;
use project::catalog::{supported_block_models, supported_block_type, supported_block_types};
use project::chain::Chain;
use project::project::Project;
use slint::{Model, ModelRc, SharedString, VecModel};
use std::rc::Rc;

pub(crate) use crate::project_view_assets::{load_screenshot_image, load_thumbnail_image};
pub(crate) use crate::project_view_tooltips::{chain_inputs_tooltip, chain_outputs_tooltip};

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

pub(crate) fn replace_project_chains(
    model: &Rc<VecModel<ProjectChainItem>>,
    project: &Project,
    input_devices: &[AudioDeviceDescriptor],
    output_devices: &[AudioDeviceDescriptor],
    io_bindings: &[IoBinding],
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
                subtitle: chain_routing_summary(chain, io_bindings).into(),
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
                    let binding_name = chain_io_chip_label_from_bindings(chain, io_bindings, true);
                    if binding_name.is_empty() {
                        // #716: device endpoints resolve from the binding
                        // registry (never from block `entries`).
                        let (resolved_inputs, _) =
                            engine::runtime_endpoints::resolve_chain_io(chain, io_bindings);
                        let input_chs: Vec<usize> = resolved_inputs
                            .iter()
                            .flat_map(|e| e.channels.iter().copied())
                            .collect();
                        chain_endpoint_label("In", &input_chs).into()
                    } else {
                        binding_name.into()
                    }
                },
                input_tooltip: chain_inputs_tooltip(chain, project, input_devices, io_bindings)
                    .into(),
                output_label: {
                    let binding_name = chain_io_chip_label_from_bindings(chain, io_bindings, false);
                    if binding_name.is_empty() {
                        // #716: device endpoints resolve from the binding
                        // registry (never from block `entries`).
                        let (_, resolved_outputs) =
                            engine::runtime_endpoints::resolve_chain_io(chain, io_bindings);
                        let output_chs: Vec<usize> = resolved_outputs
                            .iter()
                            .flat_map(|e| e.channels.iter().copied())
                            .collect();
                        chain_endpoint_label("Out", &output_chs).into()
                    } else {
                        binding_name.into()
                    }
                },
                output_tooltip: chain_outputs_tooltip(chain, project, output_devices, io_bindings)
                    .into(),
                latency_ms,
                volume: chain.volume.round() as i32,
                // Issue #496: meters default to SILENT until the GUI
                // timer subscribes & polls (engine::output_meter).
                meter_in_dbfs: engine::output_meter::SILENT_DBFS,
                meter_out_dbfs: engine::output_meter::SILENT_DBFS,
                // #771: the DI meter row starts silent; the timer fills it
                // from the isolated playback's own peaks while the DI plays.
                di_meter: crate::StreamMeter {
                    in_dbfs: engine::output_meter::SILENT_DBFS,
                    out_dbfs: engine::output_meter::SILENT_DBFS,
                },
                // #771: the DI panel's output select — the chain's bound
                // output endpoints + the persisted pick.
                di_loop_outputs: {
                    let (labels, _) =
                        crate::di_output_options::output_labels_and_index(chain, io_bindings);
                    ModelRc::from(Rc::new(VecModel::from(
                        labels
                            .into_iter()
                            .map(SharedString::from)
                            .collect::<Vec<_>>(),
                    )))
                },
                di_output_selected_index: crate::di_output_options::output_labels_and_index(
                    chain,
                    io_bindings,
                )
                .1,
                // Issue #670: no overload until the meter timer observes
                // xruns from the running audio callback.
                audio_overload: false,
                // Per-stream meter slots. When the chain is enabled the length
                // matches the number of resolved input endpoints (one stream
                // per input runtime in the engine, per invariant #4); the
                // timer fills the live values, defaulting to SILENT here so the
                // UI renders the right number of (silent) bars on first paint.
                // When disabled the length is 0 (#750: the live graph hides).
                stream_meters: {
                    // #750: the per-stream graph is a LIVE surface — render
                    // ZERO rows while the chain is disabled so nothing shows
                    // until it is enabled. When enabled, one stream per
                    // resolved input endpoint (#716: from the binding registry,
                    // not per block `entries`), min 1 so an enabled-but-
                    // unresolved chain still shows a row.
                    let stream_count: usize = if chain.enabled {
                        engine::runtime_endpoints::resolve_chain_io(chain, io_bindings)
                            .0
                            .len()
                            .max(1)
                    } else {
                        0
                    };
                    let model: Rc<VecModel<crate::StreamMeter>> = Rc::new(VecModel::default());
                    for _ in 0..stream_count {
                        model.push(crate::StreamMeter {
                            in_dbfs: engine::output_meter::SILENT_DBFS,
                            out_dbfs: engine::output_meter::SILENT_DBFS,
                        });
                    }
                    ModelRc::from(model)
                },
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
                // #614: starts false; the meter timer updates it at ~30 Hz
                // via `ChainRuntimeState::has_di_loop`. Populated here so
                // the struct is complete on first render.
                di_loop_playing: false,
                // #614: enumerate bundled loop ids (stems under
                // <data-root>/assets/di-loops/) then append the
                // "Choose file…" sentinel. If the directory is missing or
                // empty (Task 8 ships the first loops), only the sentinel
                // appears so the user can still pick a WAV file.
                di_loop_sources: {
                    let bundled_ids = crate::di_loop_ui_sources::bundled_di_loop_ids();
                    let refs: Vec<&str> = bundled_ids.iter().map(|s| s.as_str()).collect();
                    let entries = crate::di_loop_ui_sources::build_di_loop_sources(&refs);
                    ModelRc::from(Rc::new(VecModel::from(
                        entries
                            .into_iter()
                            .map(SharedString::from)
                            .collect::<Vec<_>>(),
                    )))
                },
                di_loop_selected_index: -1, // #661: refreshed by meter timer
                // #323: the looper rows and the header tint start empty and
                // are refreshed by the meter timer from the live runtimes.
                loopers: ModelRc::from(Rc::new(VecModel::from(Vec::new()))),
                looper_active: false,
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
