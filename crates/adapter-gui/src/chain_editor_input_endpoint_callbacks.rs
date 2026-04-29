//! Inline input-endpoint editor callbacks for the per-instance
//! `ChainEditorWindow`.
//!
//! Wires `on_input_select_device`, `on_input_toggle_channel`,
//! `on_input_select_mode`, `on_input_cancel`, `on_input_save`. Save commits
//! the input groups draft into the project chain (replacing all `Input(_)`
//! blocks of that chain) or, when an `IoBlockInsertDraft` is active,
//! inserts a new input block at `before_index`. Both paths resync the live
//! runtime and refresh the chain rows.

use std::cell::RefCell;
use std::rc::Rc;

use slint::{ComponentHandle, VecModel};

use domain::ids::{BlockId, DeviceId};
use infra_cpal::{AudioDeviceDescriptor, ProjectRuntimeController};
use project::block::{AudioBlock, AudioBlockKind, InputBlock, InputEntry};

use crate::chain_editor::input_mode_from_index;
use crate::io_groups::apply_chain_io_groups;
use crate::project_ops::sync_project_dirty;
use crate::project_view::replace_project_chains;
use crate::state::{ChainDraft, IoBlockInsertDraft, ProjectSession};
use crate::sync_live_chain_runtime;
use crate::{AppWindow, ChainEditorWindow, ProjectChainItem};

#[allow(clippy::too_many_arguments)]
pub(crate) fn wire(
    editor_window: &ChainEditorWindow,
    weak_window: slint::Weak<AppWindow>,
    chain_draft: Rc<RefCell<Option<ChainDraft>>>,
    project_session: Rc<RefCell<Option<ProjectSession>>>,
    project_chains: Rc<VecModel<ProjectChainItem>>,
    project_runtime: Rc<RefCell<Option<ProjectRuntimeController>>>,
    saved_project_snapshot: Rc<RefCell<Option<String>>>,
    project_dirty: Rc<RefCell<bool>>,
    input_chain_devices: Rc<RefCell<Vec<AudioDeviceDescriptor>>>,
    output_chain_devices: Rc<RefCell<Vec<AudioDeviceDescriptor>>>,
    io_block_insert_draft: Rc<RefCell<Option<IoBlockInsertDraft>>>,
    auto_save: bool,
) {
    // inline input editor: on_input_select_device
    {
        let weak_window = weak_window.clone();
        editor_window.on_input_select_device(move |index| {
            if let Some(window) = weak_window.upgrade() {
                window.invoke_select_chain_input_device(index);
            }
        });
    }
    // inline input editor: on_input_toggle_channel
    {
        let weak_window = weak_window.clone();
        editor_window.on_input_toggle_channel(move |index, selected| {
            if let Some(window) = weak_window.upgrade() {
                window.invoke_toggle_chain_input_channel(index, selected);
            }
        });
    }
    // inline input editor: on_input_select_mode
    {
        let chain_draft = chain_draft.clone();
        editor_window.on_input_select_mode(move |index| {
            if let Some(draft) = chain_draft.borrow_mut().as_mut() {
                if let Some(gi) = draft.editing_input_index {
                    if let Some(input) = draft.inputs.get_mut(gi) {
                        input.mode = input_mode_from_index(index);
                    }
                }
            }
        });
    }
    // inline input editor: on_input_cancel
    {
        let weak_chain_window = editor_window.as_weak();
        let weak_window = weak_window.clone();
        let chain_draft = chain_draft.clone();
        let io_block_insert_draft = io_block_insert_draft.clone();
        let input_chain_devices = input_chain_devices.clone();
        let output_chain_devices = output_chain_devices.clone();
        editor_window.on_input_cancel(move || {
            let Some(chain_window) = weak_chain_window.upgrade() else {
                return;
            };
            chain_window.set_input_editor_status("".into());
            chain_window.set_show_input_editor(false);
            if io_block_insert_draft.borrow().is_some() {
                *io_block_insert_draft.borrow_mut() = None;
                *chain_draft.borrow_mut() = None;
                return;
            }
            let mut draft_borrow = chain_draft.borrow_mut();
            if let Some(draft) = draft_borrow.as_mut() {
                if draft.adding_new_input {
                    if let Some(idx) = draft.editing_input_index {
                        if idx < draft.inputs.len() {
                            draft.inputs.remove(idx);
                        }
                    }
                    draft.adding_new_input = false;
                    draft.editing_input_index = None;
                    if let Some(window) = weak_window.upgrade() {
                        apply_chain_io_groups(
                            &window,
                            &chain_window,
                            draft,
                            &*input_chain_devices.borrow(),
                            &*output_chain_devices.borrow(),
                        );
                    }
                }
            }
        });
    }
    // inline input editor: on_input_save
    {
        let weak_window = weak_window.clone();
        let weak_chain_window = editor_window.as_weak();
        let chain_draft = chain_draft.clone();
        let io_block_insert_draft = io_block_insert_draft.clone();
        let project_session = project_session.clone();
        let project_chains = project_chains.clone();
        let project_runtime = project_runtime.clone();
        let saved_project_snapshot = saved_project_snapshot.clone();
        let project_dirty = project_dirty.clone();
        let input_chain_devices = input_chain_devices.clone();
        let output_chain_devices = output_chain_devices.clone();
        editor_window.on_input_save(move || {
            let Some(window) = weak_window.upgrade() else {
                return;
            };
            let Some(chain_window) = weak_chain_window.upgrade() else {
                return;
            };
            let io_insert = io_block_insert_draft.borrow().clone();
            if let Some(io_draft) = io_insert {
                if io_draft.kind == "input" {
                    let input_group = {
                        let draft_borrow = chain_draft.borrow();
                        let Some(draft) = draft_borrow.as_ref() else {
                            *io_block_insert_draft.borrow_mut() = None;
                            chain_window.set_show_input_editor(false);
                            return;
                        };
                        let Some(ig) = draft.inputs.first().cloned() else {
                            *io_block_insert_draft.borrow_mut() = None;
                            chain_window.set_show_input_editor(false);
                            return;
                        };
                        ig
                    };
                    if input_group.device_id.is_none() || input_group.channels.is_empty() {
                        chain_window
                            .set_input_editor_status("Selecione dispositivo e canais.".into());
                        return;
                    }
                    let chain_index = io_draft.chain_index;
                    let before_index = io_draft.before_index;
                    *io_block_insert_draft.borrow_mut() = None;
                    *chain_draft.borrow_mut() = None;
                    let mut session_borrow = project_session.borrow_mut();
                    let Some(session) = session_borrow.as_mut() else {
                        chain_window.set_show_input_editor(false);
                        return;
                    };
                    let Some(chain) = session.project.chains.get_mut(chain_index) else {
                        chain_window.set_show_input_editor(false);
                        return;
                    };
                    let real_chain_id = chain.id.clone();
                    let input_block = AudioBlock {
                        id: BlockId::generate_for_chain(&real_chain_id),
                        enabled: true,
                        kind: AudioBlockKind::Input(InputBlock {
                            model: "standard".to_string(),
                            entries: vec![InputEntry {
                                device_id: DeviceId(
                                    input_group.device_id.clone().unwrap_or_default(),
                                ),
                                mode: input_group.mode,
                                channels: input_group.channels.clone(),
                            }],
                        }),
                    };
                    let insert_pos = before_index.min(chain.blocks.len());
                    chain.blocks.insert(insert_pos, input_block);
                    if let Err(error) =
                        sync_live_chain_runtime(&project_runtime, session, &real_chain_id)
                    {
                        eprintln!("io block insert error: {error}");
                    }
                    replace_project_chains(
                        &project_chains,
                        &session.project,
                        &*input_chain_devices.borrow(),
                        &*output_chain_devices.borrow(),
                    );
                    sync_project_dirty(
                        &window,
                        session,
                        &saved_project_snapshot,
                        &project_dirty,
                        auto_save,
                    );
                    chain_window.set_input_editor_status("".into());
                    chain_window.set_show_input_editor(false);
                    return;
                }
            }
            let mut draft_borrow = chain_draft.borrow_mut();
            let Some(draft) = draft_borrow.as_mut() else {
                chain_window.set_show_input_editor(false);
                return;
            };
            let Some(gi) = draft.editing_input_index else {
                chain_window.set_show_input_editor(false);
                return;
            };
            let Some(input_group) = draft.inputs.get(gi) else {
                chain_window.set_show_input_editor(false);
                return;
            };
            if input_group.device_id.is_none() || input_group.channels.is_empty() {
                chain_window.set_input_editor_status("Selecione dispositivo e canais.".into());
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
                let new_input_blocks: Vec<AudioBlock> = draft
                    .inputs
                    .iter()
                    .enumerate()
                    .map(|(i, ig)| AudioBlock {
                        id: BlockId(format!("{}:input:{}", chain.id.0, i)),
                        enabled: true,
                        kind: AudioBlockKind::Input(InputBlock {
                            model: "standard".to_string(),
                            entries: vec![InputEntry {
                                device_id: DeviceId(ig.device_id.clone().unwrap_or_default()),
                                mode: ig.mode,
                                channels: ig.channels.clone(),
                            }],
                        }),
                    })
                    .collect();
                let non_input_blocks: Vec<AudioBlock> = chain
                    .blocks
                    .iter()
                    .filter(|b| !matches!(&b.kind, AudioBlockKind::Input(_)))
                    .cloned()
                    .collect();
                let mut all_blocks =
                    Vec::with_capacity(new_input_blocks.len() + non_input_blocks.len());
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
                sync_project_dirty(
                    &window,
                    session,
                    &saved_project_snapshot,
                    &project_dirty,
                    auto_save,
                );
            }
            apply_chain_io_groups(
                &window,
                &chain_window,
                draft,
                &*input_chain_devices.borrow(),
                &*output_chain_devices.borrow(),
            );
            draft.adding_new_input = false;
            chain_window.set_input_editor_status("".into());
            chain_window.set_show_input_editor(false);
        });
    }
}
