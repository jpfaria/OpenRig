//! Wiring for the standalone `ChainInputWindow` / `ChainOutputWindow`
//! save and cancel callbacks (4 callbacks total).
//!
//! Save handles two flows: I/O block insert mode (inserts a fresh InputBlock
//! or OutputBlock at the stored position via io_block_insert_draft) and edit
//! existing entries (rebuilds the chain's InputBlock/OutputBlock entries from
//! the active draft). Cancel cleans up the placeholder when adding a new
//! entry and restores the IO groups view.

use std::cell::RefCell;
use std::rc::Rc;

use slint::{ComponentHandle, ModelRc, Timer, VecModel};

use domain::ids::{BlockId, DeviceId};
use infra_cpal::{AudioDeviceDescriptor, ProjectRuntimeController};
use project::block::{
    AudioBlock, AudioBlockKind, InputBlock, InputEntry, OutputBlock, OutputEntry,
};

use crate::io_groups::{apply_chain_io_groups, build_io_group_items};
use crate::project_ops::sync_project_dirty;
use crate::project_view::replace_project_chains;
use crate::state::{ChainDraft, IoBlockInsertDraft, ProjectSession};
use crate::sync_live_chain_runtime;
use crate::{
    AppWindow, ChainEditorWindow, ChainInputGroupsWindow, ChainInputWindow,
    ChainOutputGroupsWindow, ChainOutputWindow, ProjectChainItem,
};

pub(crate) struct ChainIoSaveCtx {
    pub chain_draft: Rc<RefCell<Option<ChainDraft>>>,
    pub project_session: Rc<RefCell<Option<ProjectSession>>>,
    pub project_chains: Rc<VecModel<ProjectChainItem>>,
    pub project_runtime: Rc<RefCell<Option<ProjectRuntimeController>>>,
    pub saved_project_snapshot: Rc<RefCell<Option<String>>>,
    pub project_dirty: Rc<RefCell<bool>>,
    pub input_chain_devices: Rc<RefCell<Vec<AudioDeviceDescriptor>>>,
    pub output_chain_devices: Rc<RefCell<Vec<AudioDeviceDescriptor>>>,
    pub chain_editor_window: Rc<RefCell<Option<ChainEditorWindow>>>,
    pub io_block_insert_draft: Rc<RefCell<Option<IoBlockInsertDraft>>>,
    pub toast_timer: Rc<Timer>,
    pub auto_save: bool,
}

pub(crate) fn wire(
    window: &AppWindow,
    chain_input_window: &ChainInputWindow,
    chain_output_window: &ChainOutputWindow,
    chain_input_groups_window: &ChainInputGroupsWindow,
    chain_output_groups_window: &ChainOutputGroupsWindow,
    ctx: ChainIoSaveCtx,
) {
    let ChainIoSaveCtx {
        chain_draft,
        project_session,
        project_chains,
        project_runtime,
        saved_project_snapshot,
        project_dirty,
        input_chain_devices,
        output_chain_devices,
        chain_editor_window,
        io_block_insert_draft,
        toast_timer,
        auto_save,
    } = ctx;
    let _ = toast_timer;
    {
        let weak_window = window.as_weak();
        let weak_input_window = chain_input_window.as_weak();
        let weak_chain_window = chain_editor_window.clone();
        let weak_input_groups_window = chain_input_groups_window.as_weak();
        let chain_draft = chain_draft.clone();
        let io_block_insert_draft_for_input_save = io_block_insert_draft.clone();
        let project_session = project_session.clone();
        let project_chains = project_chains.clone();
        let project_runtime = project_runtime.clone();
        let saved_project_snapshot = saved_project_snapshot.clone();
        let project_dirty = project_dirty.clone();
        let input_chain_devices = input_chain_devices.clone();
        let output_chain_devices = output_chain_devices.clone();
        chain_input_window.on_save(move || {
            let Some(window) = weak_window.upgrade() else {
                return;
            };
            let Some(input_window) = weak_input_window.upgrade() else {
                return;
            };

            // Handle I/O block insert mode: insert a single InputBlock at the stored position
            let io_insert = io_block_insert_draft_for_input_save.borrow().clone();
            log::info!("[input_window.on_save] io_insert={:?}", io_insert.as_ref().map(|d| format!("kind={}, chain={}, before={}", d.kind, d.chain_index, d.before_index)));
            if let Some(io_draft) = io_insert {
                if io_draft.kind == "input" {
                    log::info!("[input_window.on_save] INSERTING NEW InputBlock at chain={}, before={}", io_draft.chain_index, io_draft.before_index);
                    // Extract what we need from chain_draft, then drop the borrow
                    let input_group = {
                        let draft_borrow = chain_draft.borrow();
                        let Some(draft) = draft_borrow.as_ref() else {
                            let _ = input_window.hide();
                            drop(draft_borrow);
                            *io_block_insert_draft_for_input_save.borrow_mut() = None;
                            return;
                        };
                        let Some(ig) = draft.inputs.first().cloned() else {
                            let _ = input_window.hide();
                            drop(draft_borrow);
                            *io_block_insert_draft_for_input_save.borrow_mut() = None;
                            return;
                        };
                        ig
                    };
                    if input_group.device_id.is_none() || input_group.channels.is_empty() {
                        input_window.set_status_message("Selecione dispositivo e canais.".into());
                        return;
                    }
                    let chain_index = io_draft.chain_index;
                    let before_index = io_draft.before_index;
                    // Clear drafts BEFORE touching session to avoid borrow conflicts
                    *io_block_insert_draft_for_input_save.borrow_mut() = None;
                    *chain_draft.borrow_mut() = None;
                    let mut session_borrow = project_session.borrow_mut();
                    let Some(session) = session_borrow.as_mut() else {
                        let _ = input_window.hide();
                        return;
                    };
                    let Some(chain) = session.project.chains.get_mut(chain_index) else {
                        let _ = input_window.hide();
                        return;
                    };
                    let real_chain_id = chain.id.clone();
                    let input_block = AudioBlock {
                        id: BlockId::generate_for_chain(&real_chain_id),
                        enabled: true,
                        kind: AudioBlockKind::Input(InputBlock {
                            model: "standard".to_string(),
                            entries: vec![InputEntry {
                                device_id: DeviceId(input_group.device_id.clone().unwrap_or_default()),
                                mode: input_group.mode,
                                channels: input_group.channels.clone(),
                            }],
                        }),
                    };
                    let insert_pos = before_index.min(chain.blocks.len());
                    chain.blocks.insert(insert_pos, input_block);
                    if let Err(error) = sync_live_chain_runtime(&project_runtime, session, &real_chain_id) {
                        eprintln!("io block insert error: {error}");
                    }
                    replace_project_chains(
                        &project_chains,
                        &session.project,
                        &*input_chain_devices.borrow(),
                        &*output_chain_devices.borrow(),
                    );
                    sync_project_dirty(&window, session, &saved_project_snapshot, &project_dirty, auto_save);
                    input_window.set_status_message("".into());
                    let _ = input_window.hide();
                    return;
                }
            }

            log::info!("[input_window.on_save] NORMAL FLOW — editing existing entry in InputBlock");
            let mut draft_borrow = chain_draft.borrow_mut();
            let Some(draft) = draft_borrow.as_mut() else {
                let _ = input_window.hide();
                return;
            };
            let Some(gi) = draft.editing_input_index else {
                log::warn!("[input_window.on_save] no editing_input_index set!");
                let _ = input_window.hide();
                return;
            };
            log::info!("[input_window.on_save] editing_input_index={}, draft.inputs.len={}", gi, draft.inputs.len());
            let Some(input_group) = draft.inputs.get(gi) else {
                let _ = input_window.hide();
                return;
            };
            if input_group.device_id.is_none() || input_group.channels.is_empty() {
                input_window.set_status_message("Selecione dispositivo e canais.".into());
                return;
            }
            if let Some(index) = draft.editing_index {
                let mut session_borrow = project_session.borrow_mut();
                let Some(session) = session_borrow.as_mut() else {
                    return;
                };
                let Some(chain) = session.project.chains.get_mut(index) else {
                    return;
                };
                // Rebuild chain blocks: collapse all draft input groups into a SINGLE
                // InputBlock with one entry per device. The previous shape (one
                // InputBlock per device) made each extra device appear as a separate
                // block in the canvas. Multiple entries inside one block is the
                // intended representation — runtime fans out per entry.
                let new_input_blocks: Vec<AudioBlock> = if draft.inputs.is_empty() {
                    Vec::new()
                } else {
                    let entries: Vec<InputEntry> = draft.inputs.iter().map(|ig| InputEntry {
                        device_id: DeviceId(ig.device_id.clone().unwrap_or_default()),
                        mode: ig.mode,
                        channels: ig.channels.clone(),
                    }).collect();
                    vec![AudioBlock {
                        id: BlockId(format!("{}:input", chain.id.0)),
                        enabled: true,
                        kind: AudioBlockKind::Input(InputBlock {
                            model: "standard".to_string(),
                            entries,
                        }),
                    }]
                };
                let non_input_blocks: Vec<AudioBlock> = chain.blocks.iter()
                    .filter(|b| !matches!(&b.kind, AudioBlockKind::Input(_)))
                    .cloned()
                    .collect();
                let mut all_blocks = Vec::with_capacity(new_input_blocks.len() + non_input_blocks.len());
                all_blocks.extend(new_input_blocks);
                all_blocks.extend(non_input_blocks);
                chain.blocks = all_blocks;
                let chain_id = chain.id.clone();
                if let Err(error) = sync_live_chain_runtime(&project_runtime, session, &chain_id) {
                    eprintln!("input editor save error: {error}");
                    return;
                }
                replace_project_chains(
                    &project_chains,
                    &session.project,
                    &*input_chain_devices.borrow(),
                    &*output_chain_devices.borrow(),
                );
                sync_project_dirty(&window, session, &saved_project_snapshot, &project_dirty, auto_save);
            }
            if let Some(chain_window) = weak_chain_window.borrow().as_ref() {
                apply_chain_io_groups(
                    &window,
                    chain_window,
                    draft,
                    &*input_chain_devices.borrow(),
                    &*output_chain_devices.borrow(),
                );
            }
            // Refresh input groups window if open
            if let Some(groups_window) = weak_input_groups_window.upgrade() {
                let (input_items, _) =
                    build_io_group_items(draft, &*input_chain_devices.borrow(), &*output_chain_devices.borrow());
                groups_window
                    .set_groups(ModelRc::from(Rc::new(VecModel::from(input_items))));
            }
            // Clear the adding flag on successful save
            draft.adding_new_input = false;
            input_window.set_status_message("".into());
            let _ = input_window.hide();
        });
    }
    {
        let weak_output_window = chain_output_window.as_weak();
        let chain_editor_window_for_out_cancel = chain_editor_window.clone();
        let weak_window_for_out_cancel = window.as_weak();
        let weak_output_groups_for_cancel = chain_output_groups_window.as_weak();
        let io_block_insert_draft_for_output_cancel = io_block_insert_draft.clone();
        let chain_draft_for_output_cancel = chain_draft.clone();
        let input_chain_devices_for_out_cancel = input_chain_devices.clone();
        let output_chain_devices_for_out_cancel = output_chain_devices.clone();
        chain_output_window.on_cancel(move || {
            if let Some(output_window) = weak_output_window.upgrade() {
                output_window.set_status_message("".into());
                let _ = output_window.hide();
            }
            if io_block_insert_draft_for_output_cancel.borrow().is_some() {
                *io_block_insert_draft_for_output_cancel.borrow_mut() = None;
                *chain_draft_for_output_cancel.borrow_mut() = None;
                return;
            }
            // If we were adding a new entry, remove the placeholder
            let mut draft_borrow = chain_draft_for_output_cancel.borrow_mut();
            if let Some(draft) = draft_borrow.as_mut() {
                if draft.adding_new_output {
                    if let Some(idx) = draft.editing_output_index {
                        if idx < draft.outputs.len() {
                            draft.outputs.remove(idx);
                        }
                    }
                    draft.adding_new_output = false;
                    draft.editing_output_index = None;
                    // Refresh chain editor window
                    if let Some(window) = weak_window_for_out_cancel.upgrade() {
                        if let Some(chain_window) = chain_editor_window_for_out_cancel.borrow().as_ref() {
                            apply_chain_io_groups(
                                &window,
                                chain_window,
                                draft,
                                &*input_chain_devices_for_out_cancel.borrow(),
                                &*output_chain_devices_for_out_cancel.borrow(),
                            );
                        }
                    }
                    // Refresh groups window if open
                    if let Some(groups_window) = weak_output_groups_for_cancel.upgrade() {
                        let (_, output_items) =
                            build_io_group_items(draft, &*input_chain_devices_for_out_cancel.borrow(), &*output_chain_devices_for_out_cancel.borrow());
                        groups_window
                            .set_groups(ModelRc::from(Rc::new(VecModel::from(output_items))));
                    }
                }
            }
        });
    }
    {
        let weak_input_window = chain_input_window.as_weak();
        let chain_editor_window_for_cancel = chain_editor_window.clone();
        let weak_window_for_cancel = window.as_weak();
        let weak_input_groups_for_cancel = chain_input_groups_window.as_weak();
        let io_block_insert_draft_for_input_cancel = io_block_insert_draft.clone();
        let chain_draft_for_input_cancel = chain_draft.clone();
        let input_chain_devices_for_cancel = input_chain_devices.clone();
        let output_chain_devices_for_cancel = output_chain_devices.clone();
        chain_input_window.on_cancel(move || {
            if let Some(input_window) = weak_input_window.upgrade() {
                input_window.set_status_message("".into());
                let _ = input_window.hide();
            }
            if io_block_insert_draft_for_input_cancel.borrow().is_some() {
                *io_block_insert_draft_for_input_cancel.borrow_mut() = None;
                *chain_draft_for_input_cancel.borrow_mut() = None;
                return;
            }
            // If we were adding a new entry, remove the placeholder
            let mut draft_borrow = chain_draft_for_input_cancel.borrow_mut();
            if let Some(draft) = draft_borrow.as_mut() {
                if draft.adding_new_input {
                    if let Some(idx) = draft.editing_input_index {
                        if idx < draft.inputs.len() {
                            draft.inputs.remove(idx);
                        }
                    }
                    draft.adding_new_input = false;
                    draft.editing_input_index = None;
                    // Refresh chain editor window
                    if let Some(window) = weak_window_for_cancel.upgrade() {
                        if let Some(chain_window) = chain_editor_window_for_cancel.borrow().as_ref() {
                            apply_chain_io_groups(
                                &window,
                                chain_window,
                                draft,
                                &*input_chain_devices_for_cancel.borrow(),
                                &*output_chain_devices_for_cancel.borrow(),
                            );
                        }
                    }
                    // Refresh groups window if open
                    if let Some(groups_window) = weak_input_groups_for_cancel.upgrade() {
                        let (input_items, _) =
                            build_io_group_items(draft, &*input_chain_devices_for_cancel.borrow(), &*output_chain_devices_for_cancel.borrow());
                        groups_window
                            .set_groups(ModelRc::from(Rc::new(VecModel::from(input_items))));
                    }
                }
            }
        });
    }
    {
        let weak_window = window.as_weak();
        let weak_output_window = chain_output_window.as_weak();
        let chain_editor_window_ref = chain_editor_window.clone();
        let weak_output_groups_window = chain_output_groups_window.as_weak();
        let chain_draft = chain_draft.clone();
        let io_block_insert_draft_for_output_save = io_block_insert_draft.clone();
        let project_session = project_session.clone();
        let project_chains = project_chains.clone();
        let project_runtime = project_runtime.clone();
        let saved_project_snapshot = saved_project_snapshot.clone();
        let project_dirty = project_dirty.clone();
        let input_chain_devices = input_chain_devices.clone();
        let output_chain_devices = output_chain_devices.clone();
        chain_output_window.on_save(move || {
            let Some(window) = weak_window.upgrade() else {
                return;
            };
            let Some(output_window) = weak_output_window.upgrade() else {
                return;
            };

            // Handle I/O block insert mode: insert a single OutputBlock at the stored position
            let io_insert = io_block_insert_draft_for_output_save.borrow().clone();
            if let Some(io_draft) = io_insert {
                if io_draft.kind == "output" {
                    let output_group = {
                        let draft_borrow = chain_draft.borrow();
                        let Some(draft) = draft_borrow.as_ref() else {
                            let _ = output_window.hide();
                            drop(draft_borrow);
                            *io_block_insert_draft_for_output_save.borrow_mut() = None;
                            return;
                        };
                        let Some(og) = draft.outputs.first().cloned() else {
                            let _ = output_window.hide();
                            drop(draft_borrow);
                            *io_block_insert_draft_for_output_save.borrow_mut() = None;
                            return;
                        };
                        og
                    };
                    if output_group.device_id.is_none() || output_group.channels.is_empty() {
                        output_window.set_status_message("Selecione dispositivo e canais.".into());
                        return;
                    }
                    let chain_index = io_draft.chain_index;
                    let before_index = io_draft.before_index;
                    *io_block_insert_draft_for_output_save.borrow_mut() = None;
                    *chain_draft.borrow_mut() = None;
                    let mut session_borrow = project_session.borrow_mut();
                    let Some(session) = session_borrow.as_mut() else {
                        let _ = output_window.hide();
                        return;
                    };
                    let Some(chain) = session.project.chains.get_mut(chain_index) else {
                        let _ = output_window.hide();
                        return;
                    };
                    let real_chain_id = chain.id.clone();
                    let output_block = AudioBlock {
                        id: BlockId::generate_for_chain(&real_chain_id),
                        enabled: true,
                        kind: AudioBlockKind::Output(OutputBlock {
                            model: "standard".to_string(),
                            entries: vec![OutputEntry {
                                device_id: DeviceId(output_group.device_id.clone().unwrap_or_default()),
                                mode: output_group.mode,
                                channels: output_group.channels.clone(),
                            }],
                        }),
                    };
                    let insert_pos = before_index.min(chain.blocks.len());
                    chain.blocks.insert(insert_pos, output_block);
                    if let Err(error) = sync_live_chain_runtime(&project_runtime, session, &real_chain_id) {
                        eprintln!("io block insert error: {error}");
                    }
                    replace_project_chains(
                        &project_chains,
                        &session.project,
                        &*input_chain_devices.borrow(),
                        &*output_chain_devices.borrow(),
                    );
                    sync_project_dirty(&window, session, &saved_project_snapshot, &project_dirty, auto_save);
                    output_window.set_status_message("".into());
                    let _ = output_window.hide();
                    return;
                }
            }

            let mut draft_borrow = chain_draft.borrow_mut();
            let Some(draft) = draft_borrow.as_mut() else {
                let _ = output_window.hide();
                return;
            };
            let Some(gi) = draft.editing_output_index else {
                let _ = output_window.hide();
                return;
            };
            let Some(output_group) = draft.outputs.get(gi) else {
                let _ = output_window.hide();
                return;
            };
            if output_group.device_id.is_none() || output_group.channels.is_empty() {
                output_window.set_status_message("Selecione dispositivo e canais.".into());
                return;
            }
            if let Some(index) = draft.editing_index {
                let mut session_borrow = project_session.borrow_mut();
                let Some(session) = session_borrow.as_mut() else {
                    return;
                };
                let Some(chain) = session.project.chains.get_mut(index) else {
                    return;
                };
                // Rebuild chain blocks: collapse all draft output groups into a
                // SINGLE OutputBlock with one entry per device. Mirror of the input
                // path — see the input save handler for rationale.
                let new_output_blocks: Vec<AudioBlock> = if draft.outputs.is_empty() {
                    Vec::new()
                } else {
                    let entries: Vec<OutputEntry> = draft.outputs.iter().map(|og| OutputEntry {
                        device_id: DeviceId(og.device_id.clone().unwrap_or_default()),
                        mode: og.mode,
                        channels: og.channels.clone(),
                    }).collect();
                    vec![AudioBlock {
                        id: BlockId(format!("{}:output", chain.id.0)),
                        enabled: true,
                        kind: AudioBlockKind::Output(OutputBlock {
                            model: "standard".to_string(),
                            entries,
                        }),
                    }]
                };
                let non_output_blocks: Vec<AudioBlock> = chain.blocks.iter()
                    .filter(|b| !matches!(&b.kind, AudioBlockKind::Output(_)))
                    .cloned()
                    .collect();
                let mut all_blocks = Vec::with_capacity(non_output_blocks.len() + new_output_blocks.len());
                all_blocks.extend(non_output_blocks);
                all_blocks.extend(new_output_blocks);
                chain.blocks = all_blocks;
                let chain_id = chain.id.clone();
                if let Err(error) = sync_live_chain_runtime(&project_runtime, session, &chain_id) {
                    eprintln!("output editor save error: {error}");
                    return;
                }
                replace_project_chains(
                    &project_chains,
                    &session.project,
                    &*input_chain_devices.borrow(),
                    &*output_chain_devices.borrow(),
                );
                sync_project_dirty(&window, session, &saved_project_snapshot, &project_dirty, auto_save);
            }
            if let Some(chain_window) = chain_editor_window_ref.borrow().as_ref() {
                apply_chain_io_groups(
                    &window,
                    chain_window,
                    draft,
                    &*input_chain_devices.borrow(),
                    &*output_chain_devices.borrow(),
                );
            }
            // Refresh output groups window if open
            if let Some(groups_window) = weak_output_groups_window.upgrade() {
                let (_, output_items) =
                    build_io_group_items(draft, &*input_chain_devices.borrow(), &*output_chain_devices.borrow());
                groups_window
                    .set_groups(ModelRc::from(Rc::new(VecModel::from(output_items))));
            }
            // Clear the adding flag on successful save
            draft.adding_new_output = false;
            output_window.set_status_message("".into());
            let _ = output_window.hide();
        });
    }
}
