use anyhow::{anyhow, Result};
use infra_cpal::AudioDeviceDescriptor;
use project::block::{build_audio_block_kind, schema_for_block_model, AudioBlock, AudioBlockKind};
use project::param::{ParameterDomain, ParameterSet, ParameterUnit, ParameterWidget};
use slint::{Model, ModelRc, SharedString, Timer, TimerMode, VecModel};
use std::cell::RefCell;
use std::rc::Rc;
use std::time::Duration;
use crate::{
    AppWindow, BlockEditorWindow, BlockKnobOverlay, BlockParameterItem,
    ProjectChainItem, SELECT_PATH_PREFIX, SELECT_SELECTED_BLOCK_ID,
};
use crate::state::{BlockEditorData, BlockEditorDraft, SelectOptionEditorItem};
use crate::project_ops::sync_project_dirty;
use crate::project_view::replace_project_chains;
use crate::helpers::log_gui_error;

pub(crate) fn build_knob_overlays(knob_layout: &[block_core::KnobLayoutEntry], param_items: &[BlockParameterItem]) -> Vec<BlockKnobOverlay> {
    knob_layout
        .iter()
        .map(|info| {
            let found = param_items
                .iter()
                .find(|p| p.path.as_str() == info.param_key);
            let value = found.map(|p| p.numeric_value).unwrap_or(info.min);
            let label = found
                .map(|p| p.label.to_string().to_uppercase())
                .unwrap_or_else(|| info.param_key.to_uppercase());
            BlockKnobOverlay {
                path: info.param_key.into(),
                label: label.into(),
                svg_cx: info.svg_cx,
                svg_cy: info.svg_cy,
                svg_r: info.svg_r,
                value,
                min_val: info.min,
                max_val: info.max,
                step: info.step,
            }
        })
        .collect()
}

#[allow(clippy::too_many_arguments)]
pub(crate) fn schedule_block_editor_persist(
    timer: &Rc<Timer>,
    window_weak: slint::Weak<AppWindow>,
    block_editor_draft: Rc<RefCell<Option<BlockEditorDraft>>>,
    block_parameter_items: Rc<VecModel<BlockParameterItem>>,
    project_session: Rc<RefCell<Option<crate::state::ProjectSession>>>,
    project_chains: Rc<VecModel<ProjectChainItem>>,
    project_runtime: Rc<RefCell<Option<infra_cpal::ProjectRuntimeController>>>,
    saved_project_snapshot: Rc<RefCell<Option<String>>>,
    project_dirty: Rc<RefCell<bool>>,
    input_chain_devices: Rc<RefCell<Vec<AudioDeviceDescriptor>>>,
    output_chain_devices: Rc<RefCell<Vec<AudioDeviceDescriptor>>>,
    context: &'static str,
    auto_save: bool,
) {
    timer.stop();
    timer.start(TimerMode::SingleShot, Duration::from_millis(30), move || {
        let Some(window) = window_weak.upgrade() else {
            return;
        };
        let Some(draft) = block_editor_draft.borrow().clone() else {
            return;
        };
        if draft.block_index.is_none() {
            return;
        }
        let devs_in = input_chain_devices.borrow();
        let devs_out = output_chain_devices.borrow();
        if let Err(error) = persist_block_editor_draft(
            &window,
            &draft,
            &block_parameter_items,
            &project_session,
            &project_chains,
            &project_runtime,
            &saved_project_snapshot,
            &project_dirty,
            &*devs_in,
            &*devs_out,
            false,
            auto_save,
        ) {
            log::error!("[adapter-gui] {context}: {error}");
            window.set_block_drawer_status_message(error.to_string().into());
        }
    });
}

#[allow(clippy::too_many_arguments)]
pub(crate) fn schedule_block_editor_persist_for_block_win(
    timer: &Rc<Timer>,
    block_win_weak: slint::Weak<BlockEditorWindow>,
    main_win_weak: slint::Weak<AppWindow>,
    block_editor_draft: Rc<RefCell<Option<BlockEditorDraft>>>,
    block_parameter_items: Rc<VecModel<BlockParameterItem>>,
    project_session: Rc<RefCell<Option<crate::state::ProjectSession>>>,
    project_chains: Rc<VecModel<ProjectChainItem>>,
    project_runtime: Rc<RefCell<Option<infra_cpal::ProjectRuntimeController>>>,
    saved_project_snapshot: Rc<RefCell<Option<String>>>,
    project_dirty: Rc<RefCell<bool>>,
    input_chain_devices: Rc<RefCell<Vec<AudioDeviceDescriptor>>>,
    output_chain_devices: Rc<RefCell<Vec<AudioDeviceDescriptor>>>,
    context: &'static str,
    auto_save: bool,
) {
    timer.stop();
    timer.start(TimerMode::SingleShot, Duration::from_millis(30), move || {
        // Only persist if the block window is still alive
        if block_win_weak.upgrade().is_none() {
            return;
        }
        let Some(main_window) = main_win_weak.upgrade() else {
            return;
        };
        let Some(draft) = block_editor_draft.borrow().clone() else {
            return;
        };
        if draft.block_index.is_none() {
            return;
        }
        let devs_in = input_chain_devices.borrow();
        let devs_out = output_chain_devices.borrow();
        if let Err(error) = persist_block_editor_draft(
            &main_window,
            &draft,
            &block_parameter_items,
            &project_session,
            &project_chains,
            &project_runtime,
            &saved_project_snapshot,
            &project_dirty,
            &*devs_in,
            &*devs_out,
            false,
            auto_save,
        ) {
            log::error!("[adapter-gui] {context}: {error}");
            main_window.set_block_drawer_status_message(error.to_string().into());
        }
    });
}

pub(crate) fn block_editor_data(block: &AudioBlock) -> Option<BlockEditorData> {
    block_editor_data_with_selected(block, None)
}

pub(crate) fn block_editor_data_with_selected(
    block: &AudioBlock,
    selected_option_block_id: Option<&str>,
) -> Option<BlockEditorData> {
    match &block.kind {
        AudioBlockKind::Select(select) => {
            let selected = selected_option_block_id
                .and_then(|selected_id| {
                    select
                        .options
                        .iter()
                        .find(|option| option.id.0 == selected_id)
                })
                .or_else(|| select.selected_option())?;
            let model = selected.model_ref()?;
            Some(BlockEditorData {
                effect_type: model.effect_type.to_string(),
                model_id: model.model.to_string(),
                params: model.params.clone(),
                enabled: block.enabled,
                is_select: true,
                select_options: select
                    .options
                    .iter()
                    .filter_map(|option| {
                        let model = option.model_ref()?;
                        let label = schema_for_block_model(model.effect_type, model.model)
                            .map(|schema| schema.display_name)
                            .unwrap_or_else(|_| model.model.to_string());
                        Some(SelectOptionEditorItem {
                            block_id: option.id.0.clone(),
                            label,
                        })
                    })
                    .collect(),
                selected_select_option_block_id: Some(select.selected_block_id.0.clone()),
            })
        }
        _ => block.model_ref().map(|model| BlockEditorData {
            effect_type: model.effect_type.to_string(),
            model_id: model.model.to_string(),
            params: model.params.clone(),
            enabled: block.enabled,
            is_select: false,
            select_options: Vec::new(),
            selected_select_option_block_id: None,
        }),
    }
}

pub(crate) fn block_parameter_items_for_editor(data: &BlockEditorData) -> Vec<BlockParameterItem> {
    let mut items = Vec::new();
    if !data.select_options.is_empty() {
        let option_labels = data
            .select_options
            .iter()
            .map(|option| SharedString::from(option.label.as_str()))
            .collect::<Vec<_>>();
        let option_values = data
            .select_options
            .iter()
            .map(|option| SharedString::from(option.block_id.as_str()))
            .collect::<Vec<_>>();
        let selected_option_index = data
            .selected_select_option_block_id
            .as_ref()
            .and_then(|selected| {
                data.select_options
                    .iter()
                    .position(|option| &option.block_id == selected)
            })
            .map(|index| index as i32)
            .unwrap_or(0);
        items.push(BlockParameterItem {
            path: SELECT_SELECTED_BLOCK_ID.into(),
            label: "Modelo ativo".into(),
            group: "Select".into(),
            widget_kind: "enum".into(),
            unit_text: "".into(),
            value_text: data
                .selected_select_option_block_id
                .clone()
                .unwrap_or_default()
                .into(),
            numeric_value: 0.0,
            numeric_min: 0.0,
            numeric_max: 1.0,
            numeric_step: 0.0,
            numeric_integer: false,
            bool_value: false,
            selected_option_index,
            option_labels: ModelRc::from(Rc::new(VecModel::from(option_labels))),
            option_values: ModelRc::from(Rc::new(VecModel::from(option_values))),
            file_extensions: ModelRc::from(Rc::new(VecModel::from(Vec::<SharedString>::new()))),
            optional: false,
            allow_empty: false,
        });
    }
    items.extend(block_parameter_items_for_model(
        &data.effect_type,
        &data.model_id,
        &data.params,
    ));
    items
}

pub(crate) fn block_parameter_items_for_model(
    effect_type: &str,
    model_id: &str,
    params: &ParameterSet,
) -> Vec<BlockParameterItem> {
    let Ok(schema) = schema_for_block_model(effect_type, model_id) else {
        return Vec::new();
    };
    schema
        .parameters
        .iter()
        .filter(|spec| spec.path != "enabled")
        .map(|spec| {
            let current = params
                .get(&spec.path)
                .cloned()
                .or_else(|| spec.default_value.clone())
                .unwrap_or(domain::value_objects::ParameterValue::Null);
            let (numeric_value, numeric_min, numeric_max, numeric_step) = match &spec.domain {
                ParameterDomain::IntRange { min, max, .. } => (
                    current.as_i64().unwrap_or(*min) as f32,
                    *min as f32,
                    *max as f32,
                    match &spec.domain {
                        ParameterDomain::IntRange { step, .. } => *step as f32,
                        _ => 1.0,
                    },
                ),
                ParameterDomain::FloatRange { min, max, .. } => (
                    current.as_f32().unwrap_or(*min),
                    *min,
                    *max,
                    match &spec.domain {
                        ParameterDomain::FloatRange { step, .. } => *step,
                        _ => 0.0,
                    },
                ),
                _ => (0.0, 0.0, 1.0, 0.0),
            };
            let (option_labels, option_values, selected_option_index, file_extensions) = match &spec
                .domain
            {
                ParameterDomain::Enum { options } => {
                    let labels = options
                        .iter()
                        .map(|option| SharedString::from(option.label.as_str()))
                        .collect::<Vec<_>>();
                    let values = options
                        .iter()
                        .map(|option| SharedString::from(option.value.as_str()))
                        .collect::<Vec<_>>();
                    let selected = current
                        .as_str()
                        .and_then(|value| options.iter().position(|option| option.value == value))
                        .map(|index| index as i32)
                        .unwrap_or(0);
                    (
                        ModelRc::from(Rc::new(VecModel::from(labels))),
                        ModelRc::from(Rc::new(VecModel::from(values))),
                        selected,
                        ModelRc::from(Rc::new(VecModel::from(Vec::<SharedString>::new()))),
                    )
                }
                ParameterDomain::FilePath { extensions } => {
                    let values = extensions
                        .iter()
                        .map(|value| SharedString::from(value.as_str()))
                        .collect::<Vec<_>>();
                    (
                        ModelRc::from(Rc::new(VecModel::from(Vec::<SharedString>::new()))),
                        ModelRc::from(Rc::new(VecModel::from(Vec::<SharedString>::new()))),
                        -1,
                        ModelRc::from(Rc::new(VecModel::from(values))),
                    )
                }
                _ => (
                    ModelRc::from(Rc::new(VecModel::from(Vec::<SharedString>::new()))),
                    ModelRc::from(Rc::new(VecModel::from(Vec::<SharedString>::new()))),
                    -1,
                    ModelRc::from(Rc::new(VecModel::from(Vec::<SharedString>::new()))),
                ),
            };
            BlockParameterItem {
                path: spec.path.clone().into(),
                label: spec.label.to_uppercase().into(),
                group: spec.group.clone().unwrap_or_default().into(),
                widget_kind: match &spec.widget {
                    ParameterWidget::MultiSlider | ParameterWidget::CurveEditor { .. } => "",
                    _ => match &spec.domain {
                        ParameterDomain::Bool => "bool",
                        ParameterDomain::IntRange { min, max, step } => {
                            numeric_widget_kind(*min as f32, *max as f32, *step as f32, true)
                        }
                        ParameterDomain::FloatRange { min, max, step } => {
                            numeric_widget_kind(*min, *max, *step, false)
                        }
                        ParameterDomain::Enum { .. } => "enum",
                        ParameterDomain::Text => "text",
                        ParameterDomain::FilePath { .. } => "path",
                    },
                }
                .into(),
                unit_text: unit_label(&spec.unit).into(),
                value_text: match current {
                    domain::value_objects::ParameterValue::String(ref value) => {
                        value.clone().into()
                    }
                    domain::value_objects::ParameterValue::Int(value) => value.to_string().into(),
                    domain::value_objects::ParameterValue::Float(value) => {
                        format!("{value:.2}").into()
                    }
                    domain::value_objects::ParameterValue::Bool(value) => {
                        if value {
                            "true".into()
                        } else {
                            "false".into()
                        }
                    }
                    domain::value_objects::ParameterValue::Null => "".into(),
                },
                numeric_value,
                numeric_min,
                numeric_max,
                numeric_step,
                numeric_integer: matches!(&spec.domain, ParameterDomain::IntRange { .. }),
                bool_value: current.as_bool().unwrap_or(false),
                selected_option_index,
                option_labels,
                option_values,
                file_extensions,
                optional: spec.optional,
                allow_empty: spec.allow_empty,
            }
        })
        .collect()
}

pub(crate) fn set_block_parameter_text(model: &Rc<VecModel<BlockParameterItem>>, path: &str, value: &str) {
    for index in 0..model.row_count() {
        if let Some(mut row) = model.row_data(index) {
            if row.path.as_str() == path {
                row.value_text = value.into();
                model.set_row_data(index, row);
                break;
            }
        }
    }
}

pub(crate) fn set_block_parameter_bool(model: &Rc<VecModel<BlockParameterItem>>, path: &str, value: bool) {
    for index in 0..model.row_count() {
        if let Some(mut row) = model.row_data(index) {
            if row.path.as_str() == path {
                row.bool_value = value;
                model.set_row_data(index, row);
                break;
            }
        }
    }
}

pub(crate) fn set_block_parameter_number(model: &Rc<VecModel<BlockParameterItem>>, path: &str, value: f32) {
    for index in 0..model.row_count() {
        if let Some(mut row) = model.row_data(index) {
            if row.path.as_str() == path {
                let quantized = quantize_numeric_value(
                    value,
                    row.numeric_min,
                    row.numeric_max,
                    row.numeric_step,
                    row.numeric_integer,
                );
                row.numeric_value = quantized;
                row.value_text = if row.numeric_integer {
                    format!("{:.0}", quantized.round()).into()
                } else {
                    format!("{quantized:.2}").into()
                };
                model.set_row_data(index, row);
                break;
            }
        }
    }
}

#[allow(clippy::too_many_arguments)]
pub(crate) fn persist_block_editor_draft(
    window: &AppWindow,
    draft: &BlockEditorDraft,
    block_parameter_items: &Rc<VecModel<BlockParameterItem>>,
    project_session: &Rc<RefCell<Option<crate::state::ProjectSession>>>,
    project_chains: &Rc<VecModel<ProjectChainItem>>,
    project_runtime: &Rc<RefCell<Option<infra_cpal::ProjectRuntimeController>>>,
    saved_project_snapshot: &Rc<RefCell<Option<String>>>,
    project_dirty: &Rc<RefCell<bool>>,
    input_chain_devices: &[AudioDeviceDescriptor],
    output_chain_devices: &[AudioDeviceDescriptor],
    close_after_save: bool,
    auto_save: bool,
) -> Result<()> {
    let params =
        block_parameter_values(block_parameter_items, &draft.effect_type, &draft.model_id)?;
    log::info!("[persist] effect_type='{}', model_id='{}', close_after_save={}, params:", draft.effect_type, draft.model_id, close_after_save);
    for (path, value) in params.values.iter() {
        log::info!("[persist]   {} = {:?}", path, value);
    }
    let selected_select_option_block_id = if draft.is_select {
        Some(
            internal_block_parameter_value(block_parameter_items, SELECT_SELECTED_BLOCK_ID)
                .ok_or_else(|| anyhow!("Seleção do select inválida."))?,
        )
    } else {
        None
    };
    let mut session_borrow = project_session.borrow_mut();
    let session = session_borrow
        .as_mut()
        .ok_or_else(|| anyhow!("Nenhum projeto carregado."))?;
    let chain_id = {
        let chain = session
            .project
            .chains
            .get_mut(draft.chain_index)
            .ok_or_else(|| anyhow!("Chain inválida."))?;
        if let Some(block_index) = draft.block_index {
            let block = chain
                .blocks
                .get_mut(block_index)
                .ok_or_else(|| anyhow!("Block inválido."))?;
            block.enabled = draft.enabled;
            if draft.is_select {
                let AudioBlockKind::Select(select) = &mut block.kind else {
                    return Err(anyhow!("Block selecionado não é um select."));
                };
                let selected_option_block_id = selected_select_option_block_id
                    .as_ref()
                    .expect("select option id should exist");
                let select_family = select
                    .options
                    .iter()
                    .find_map(|option| option.model_ref().map(|model| model.effect_type.to_string()))
                    .ok_or_else(|| anyhow!("Select sem opções válidas."))?;
                if select_family != draft.effect_type {
                    return Err(anyhow!(
                        "Select só aceita opções do tipo '{}'.",
                        select_family
                    ));
                }
                select.selected_block_id = domain::ids::BlockId(selected_option_block_id.clone());
                let option = select
                    .options
                    .iter_mut()
                    .find(|option| option.id.0 == *selected_option_block_id)
                    .ok_or_else(|| anyhow!("Opção ativa do select não existe."))?;
                let option_id = option.id.clone();
                let option_enabled = option.enabled;
                option.kind = build_audio_block_kind(&draft.effect_type, &draft.model_id, params)
                    .map_err(|error| anyhow!(error))?;
                option.id = option_id;
                option.enabled = option_enabled;
            } else {
                let block_id = block.id.clone();
                block.kind = build_audio_block_kind(&draft.effect_type, &draft.model_id, params)
                    .map_err(|error| anyhow!(error))?;
                block.id = block_id;
            }
        } else {
            let kind = build_audio_block_kind(&draft.effect_type, &draft.model_id, params)
                .map_err(|error| anyhow!(error))?;
            let insert_index = draft.before_index.min(chain.blocks.len());
            log::info!("[persist] INSERT new block at index={}, effect_type='{}', model_id='{}'", insert_index, draft.effect_type, draft.model_id);
            chain.blocks.insert(
                insert_index,
                AudioBlock {
                    id: domain::ids::BlockId::generate_for_chain(&chain.id),
                    enabled: draft.enabled,
                    kind,
                },
            );
            log::info!("[persist] chain after insert has {} blocks:", chain.blocks.len());
            for (i, b) in chain.blocks.iter().enumerate() {
                log::info!("[persist]   [{}] id='{}' kind={}", i, b.id.0, b.model_ref().map(|m| format!("{}/{}", m.effect_type, m.model)).unwrap_or_else(|| "io/insert".to_string()));
            }
        }
        chain.id.clone()
    };
    if let Err(error) = crate::sync_live_chain_runtime(project_runtime, session, &chain_id) {
        log_gui_error("block-drawer.persist", &error);
        return Err(error);
    }
    replace_project_chains(
        project_chains,
        &session.project,
        input_chain_devices,
        output_chain_devices,
    );
    sync_project_dirty(window, session, saved_project_snapshot, project_dirty, auto_save);
    if close_after_save {
        window.set_show_block_drawer(false);
        window.set_show_block_type_picker(false);
        window.set_block_drawer_selected_model_index(-1);
        window.set_block_drawer_selected_type_index(-1);
    }
    window.set_block_drawer_status_message("".into());
    window.set_status_message("".into());
    Ok(())
}

pub(crate) fn quantize_numeric_value(value: f32, min: f32, max: f32, step: f32, integer: bool) -> f32 {
    let mut clamped = value.clamp(min, max);
    if step > 0.0 {
        let snapped_steps = ((clamped - min) / step).round();
        clamped = min + (snapped_steps * step);
        clamped = clamped.clamp(min, max);
    }
    if integer {
        clamped.round()
    } else {
        clamped
    }
}

pub(crate) fn numeric_widget_kind(min: f32, max: f32, step: f32, integer: bool) -> &'static str {
    if step > 0.0 && max > min {
        let steps = ((max - min) / step).round();
        if steps <= 24.0 {
            return "stepper";
        }
    }
    let _ = integer;
    "slider"
}

pub(crate) fn set_block_parameter_option(
    model: &Rc<VecModel<BlockParameterItem>>,
    path: &str,
    selected_index: i32,
) {
    for index in 0..model.row_count() {
        if let Some(mut row) = model.row_data(index) {
            if row.path.as_str() == path {
                row.selected_option_index = selected_index;
                if selected_index >= 0 {
                    if let Some(value) = row.option_values.row_data(selected_index as usize) {
                        row.value_text = value;
                    }
                }
                model.set_row_data(index, row);
                break;
            }
        }
    }
}

pub(crate) fn block_parameter_extensions(model: &Rc<VecModel<BlockParameterItem>>, path: &str) -> Vec<String> {
    for index in 0..model.row_count() {
        if let Some(row) = model.row_data(index) {
            if row.path.as_str() == path {
                let mut values = Vec::new();
                for ext_index in 0..row.file_extensions.row_count() {
                    if let Some(ext) = row.file_extensions.row_data(ext_index) {
                        values.push(ext.to_string());
                    }
                }
                return values;
            }
        }
    }
    Vec::new()
}

pub(crate) fn block_parameter_values(
    model: &Rc<VecModel<BlockParameterItem>>,
    effect_type: &str,
    model_id: &str,
) -> Result<ParameterSet> {
    let schema = schema_for_block_model(effect_type, model_id).map_err(|error| anyhow!(error))?;
    let mut params = ParameterSet::default();
    for index in 0..model.row_count() {
        let Some(row) = model.row_data(index) else {
            continue;
        };
        if row.path.as_str().starts_with(SELECT_PATH_PREFIX) {
            continue;
        }
        let value = match row.widget_kind.as_str() {
            "bool" => domain::value_objects::ParameterValue::Bool(row.bool_value),
            "int" => domain::value_objects::ParameterValue::Int(row.numeric_value.round() as i64),
            "float" => domain::value_objects::ParameterValue::Float(row.numeric_value),
            "slider" => {
                if row.numeric_integer {
                    domain::value_objects::ParameterValue::Int(row.numeric_value.round() as i64)
                } else {
                    domain::value_objects::ParameterValue::Float(row.numeric_value)
                }
            }
            "stepper" => {
                if row.numeric_integer {
                    domain::value_objects::ParameterValue::Int(row.numeric_value.round() as i64)
                } else {
                    domain::value_objects::ParameterValue::Float(row.numeric_value)
                }
            }
            "enum" => {
                if row.selected_option_index < 0 {
                    return Err(anyhow!("Selecione uma opção para {}", row.label));
                }
                let selected = row
                    .option_values
                    .row_data(row.selected_option_index as usize)
                    .ok_or_else(|| anyhow!("Seleção inválida para {}", row.label))?;
                domain::value_objects::ParameterValue::String(selected.to_string())
            }
            "text" | "path" => {
                let value = row.value_text.to_string();
                if row.optional && value.trim().is_empty() {
                    domain::value_objects::ParameterValue::Null
                } else {
                    domain::value_objects::ParameterValue::String(value)
                }
            }
            // CurveEditor / MultiSlider params use widget_kind="" and store their
            // value in numeric_value — persist as Float.
            "" => domain::value_objects::ParameterValue::Float(row.numeric_value),
            _ => domain::value_objects::ParameterValue::Null,
        };
        params.insert(row.path.as_str(), value);
    }
    params
        .normalized_against(&schema)
        .map_err(|error| anyhow!(error))
}

pub(crate) fn internal_block_parameter_value(
    model: &Rc<VecModel<BlockParameterItem>>,
    path: &str,
) -> Option<String> {
    for index in 0..model.row_count() {
        let Some(row) = model.row_data(index) else {
            continue;
        };
        if row.path.as_str() != path {
            continue;
        }
        if row.selected_option_index >= 0 {
            if let Some(value) = row.option_values.row_data(row.selected_option_index as usize) {
                return Some(value.to_string());
            }
        }
        return Some(row.value_text.to_string());
    }
    None
}

pub(crate) fn build_params_from_items(items: &Rc<VecModel<BlockParameterItem>>) -> ParameterSet {
    let mut params = ParameterSet::default();
    for i in 0..items.row_count() {
        if let Some(item) = items.row_data(i) {
            if !item.path.is_empty() {
                params.insert(
                    item.path.to_string(),
                    domain::value_objects::ParameterValue::Float(item.numeric_value),
                );
            }
        }
    }
    params
}

pub(crate) fn unit_label(unit: &ParameterUnit) -> &'static str {
    match unit {
        ParameterUnit::None => "",
        ParameterUnit::Decibels => "dB",
        ParameterUnit::Hertz => "Hz",
        ParameterUnit::Milliseconds => "ms",
        ParameterUnit::Percent => "%",
        ParameterUnit::Ratio => "Ratio",
        ParameterUnit::Semitones => "st",
    }
}

#[cfg(test)]
mod tests {
    use super::{
        block_editor_data, block_parameter_items_for_editor,
        numeric_widget_kind, quantize_numeric_value,
    };
    use crate::SELECT_SELECTED_BLOCK_ID;
    use domain::ids::BlockId;
    use project::catalog::supported_block_models;
    use project::block::{
        AudioBlock, AudioBlockKind, CoreBlock, SelectBlock,
    };
    use project::param::ParameterSet;
    use slint::Model;

    #[test]
    fn quantize_numeric_value_respects_float_step_and_bounds() {
        assert_eq!(quantize_numeric_value(19.64, 0.0, 100.0, 0.5, false), 19.5);
        assert_eq!(quantize_numeric_value(101.0, 0.0, 100.0, 0.5, false), 100.0);
        assert_eq!(quantize_numeric_value(-1.0, 0.0, 100.0, 0.5, false), 0.0);
    }

    #[test]
    fn quantize_numeric_value_respects_integer_step() {
        assert_eq!(
            quantize_numeric_value(243.0, 64.0, 1024.0, 64.0, true),
            256.0
        );
        assert_eq!(
            quantize_numeric_value(96.0, 64.0, 1024.0, 64.0, true),
            128.0
        );
    }

    #[test]
    fn numeric_widget_kind_prefers_stepper_for_sparse_ranges() {
        assert_eq!(numeric_widget_kind(50.0, 70.0, 10.0, false), "stepper");
        assert_eq!(numeric_widget_kind(10.0, 100.0, 10.0, false), "stepper");
    }

    #[test]
    fn numeric_widget_kind_uses_slider_for_dense_ranges() {
        assert_eq!(numeric_widget_kind(0.0, 5.0, 0.01, false), "slider");
        assert_eq!(numeric_widget_kind(1.0, 10.0, 0.1, false), "slider");
    }

    #[test]
    fn numeric_widget_kind_prefers_slider_for_large_ranges() {
        assert_eq!(numeric_widget_kind(0.0, 100.0, 0.5, false), "slider");
        assert_eq!(numeric_widget_kind(20.0, 20000.0, 1.0, false), "slider");
    }

    #[test]
    fn select_block_editor_uses_selected_option_model() {
        let delay_models = delay_model_ids();
        let first_model = delay_models.first().expect("delay catalog must not be empty");
        let second_model = delay_models.get(1).unwrap_or(first_model);
        let block = select_delay_block("chain:0:block:0", first_model.as_str(), second_model.as_str());
        let editor_data = block_editor_data(&block).expect("select should expose editor data");
        assert!(editor_data.is_select);
        assert_eq!(editor_data.effect_type, "delay");
        assert_eq!(editor_data.model_id, second_model.as_str());
        assert_eq!(editor_data.select_options.len(), 2);
        assert_eq!(
            editor_data.selected_select_option_block_id.as_deref(),
            Some("chain:0:block:0::delay_b")
        );
    }

    #[test]
    fn select_block_editor_includes_active_option_picker() {
        let delay_models = delay_model_ids();
        let first_model = delay_models.first().expect("delay catalog must not be empty");
        let second_model = delay_models.get(1).unwrap_or(first_model);
        let block = select_delay_block("chain:0:block:0", first_model.as_str(), second_model.as_str());
        let editor_data = block_editor_data(&block).expect("select should expose editor data");
        let items = block_parameter_items_for_editor(&editor_data);
        let selector = items
            .iter()
            .find(|item| item.path.as_str() == SELECT_SELECTED_BLOCK_ID)
            .expect("select editor should expose active option picker");
        assert_eq!(selector.option_values.row_count(), 2);
        assert_eq!(selector.selected_option_index, 1);
    }

    fn select_delay_block(id: &str, first_model: &str, second_model: &str) -> AudioBlock {
        AudioBlock {
            id: BlockId(id.into()),
            enabled: true,
            kind: AudioBlockKind::Select(SelectBlock {
                selected_block_id: BlockId(format!("{id}::delay_b")),
                options: vec![
                    delay_block(format!("{id}::delay_a"), first_model, 120.0),
                    delay_block(format!("{id}::delay_b"), second_model, 240.0),
                ],
            }),
        }
    }

    fn delay_block(id: impl Into<String>, model: &str, time_ms: f32) -> AudioBlock {
        let mut params = ParameterSet::default();
        params.insert("time_ms", domain::value_objects::ParameterValue::Float(time_ms));
        AudioBlock {
            id: BlockId(id.into()),
            enabled: true,
            kind: AudioBlockKind::Core(CoreBlock {
                effect_type: "delay".to_string(),
                model: model.to_string(),
                params,
            }),
        }
    }

    fn delay_model_ids() -> Vec<String> {
        supported_block_models("delay")
            .expect("delay catalog should exist")
            .into_iter()
            .map(|entry| entry.model_id)
            .collect()
    }

    // --- quantize_numeric_value edge cases ---

    #[test]
    fn quantize_numeric_value_zero_step_only_clamps() {
        assert_eq!(quantize_numeric_value(50.0, 0.0, 100.0, 0.0, false), 50.0);
        assert_eq!(quantize_numeric_value(150.0, 0.0, 100.0, 0.0, false), 100.0);
    }

    #[test]
    fn quantize_numeric_value_exact_boundary_stays() {
        assert_eq!(quantize_numeric_value(0.0, 0.0, 100.0, 10.0, false), 0.0);
        assert_eq!(quantize_numeric_value(100.0, 0.0, 100.0, 10.0, false), 100.0);
    }

    #[test]
    fn quantize_numeric_value_integer_flag_rounds() {
        assert_eq!(quantize_numeric_value(3.7, 0.0, 10.0, 0.0, true), 4.0);
        assert_eq!(quantize_numeric_value(3.2, 0.0, 10.0, 0.0, true), 3.0);
    }

    // --- numeric_widget_kind edge cases ---

    #[test]
    fn numeric_widget_kind_step_zero_returns_slider() {
        assert_eq!(numeric_widget_kind(0.0, 100.0, 0.0, false), "slider");
    }

    #[test]
    fn numeric_widget_kind_boundary_24_steps_is_stepper() {
        // exactly 24 steps: (24.0 - 0.0) / 1.0 = 24
        assert_eq!(numeric_widget_kind(0.0, 24.0, 1.0, false), "stepper");
    }

    #[test]
    fn numeric_widget_kind_25_steps_is_slider() {
        assert_eq!(numeric_widget_kind(0.0, 25.0, 1.0, false), "slider");
    }

    #[test]
    fn numeric_widget_kind_equal_min_max_returns_slider() {
        assert_eq!(numeric_widget_kind(5.0, 5.0, 1.0, false), "slider");
    }
}
