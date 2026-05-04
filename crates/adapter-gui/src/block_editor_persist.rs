//! Persistence flow for the block editor draft.
//!
//! `persist_block_editor_draft` is the synchronous commit:
//!   1. Read the parameter model into a typed `ParameterSet`.
//!   2. Build the new `AudioBlockKind` (or update the active select option).
//!   3. Mutate the project session's chain in place — replace the existing
//!      block or insert a new one at `draft.before_index`.
//!   4. Resync the live runtime, refresh chain rows, mark dirty.
//!   5. Optionally close the drawer.
//!
//! The two `schedule_*` helpers debounce the persist by 30 ms via a shared
//! `Timer`. They differ only in which weak handle gates the persist (main
//! `AppWindow` vs the secondary `BlockEditorWindow`) — both end up calling
//! `persist_block_editor_draft` once the timer fires.

use std::cell::RefCell;
use std::rc::Rc;
use std::time::Duration;

use anyhow::{anyhow, Result};
use slint::{Timer, TimerMode, VecModel};

use infra_cpal::AudioDeviceDescriptor;
use project::block::{build_audio_block_kind, AudioBlock, AudioBlockKind};

use crate::block_editor_values::{block_parameter_values, internal_block_parameter_value};
use crate::helpers::log_gui_error;
use crate::project_ops::sync_project_dirty;
use crate::project_view::replace_project_chains;
use crate::state::BlockEditorDraft;
use crate::{
    AppWindow, BlockEditorWindow, BlockParameterItem, ProjectChainItem, SELECT_SELECTED_BLOCK_ID,
};

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
    timer.start(
        TimerMode::SingleShot,
        Duration::from_millis(30),
        move || {
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
        },
    );
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
    timer.start(
        TimerMode::SingleShot,
        Duration::from_millis(30),
        move || {
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
        },
    );
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
    log::info!(
        "[persist] effect_type='{}', model_id='{}', close_after_save={}, params:",
        draft.effect_type,
        draft.model_id,
        close_after_save
    );
    for (path, value) in params.values.iter() {
        log::info!("[persist]   {} = {:?}", path, value);
    }
    let selected_select_option_block_id = if draft.is_select {
        Some(
            internal_block_parameter_value(block_parameter_items, SELECT_SELECTED_BLOCK_ID)
                .ok_or_else(|| anyhow!("{}", rust_i18n::t!("error-select-invalid")))?,
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
            .ok_or_else(|| anyhow!("{}", rust_i18n::t!("error-invalid-chain")))?;
        if let Some(block_index) = draft.block_index {
            let block = chain
                .blocks
                .get_mut(block_index)
                .ok_or_else(|| anyhow!("{}", rust_i18n::t!("error-invalid-block")))?;
            block.enabled = draft.enabled;
            if draft.is_select {
                let AudioBlockKind::Select(select) = &mut block.kind else {
                    return Err(anyhow!("{}", rust_i18n::t!("error-block-not-select")));
                };
                let selected_option_block_id = selected_select_option_block_id
                    .as_ref()
                    .expect("select option id should exist");
                let select_family = select
                    .options
                    .iter()
                    .find_map(|option| {
                        option
                            .model_ref()
                            .map(|model| model.effect_type.to_string())
                    })
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
            log::info!(
                "[persist] INSERT new block at index={}, effect_type='{}', model_id='{}'",
                insert_index,
                draft.effect_type,
                draft.model_id
            );
            chain.blocks.insert(
                insert_index,
                AudioBlock {
                    id: domain::ids::BlockId::generate_for_chain(&chain.id),
                    enabled: draft.enabled,
                    kind,
                },
            );
            log::info!(
                "[persist] chain after insert has {} blocks:",
                chain.blocks.len()
            );
            for (i, b) in chain.blocks.iter().enumerate() {
                log::info!(
                    "[persist]   [{}] id='{}' kind={}",
                    i,
                    b.id.0,
                    b.model_ref()
                        .map(|m| format!("{}/{}", m.effect_type, m.model))
                        .unwrap_or_else(|| "io/insert".to_string())
                );
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
    sync_project_dirty(
        window,
        session,
        saved_project_snapshot,
        project_dirty,
        auto_save,
    );
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
