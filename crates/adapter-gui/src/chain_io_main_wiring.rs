//! Wiring for the main window's chain-I/O picker callbacks.
//!
//! Owns the 6 callbacks that drive device + channel selection for the active
//! chain draft, plus the `configure_chain_input/output` entry points that
//! prepare the `Chain*GroupsWindow` (or the inline fullscreen panel):
//!
//! - `on_select_chain_input_device`  / `on_select_chain_output_device`
//! - `on_toggle_chain_input_channel` / `on_toggle_chain_output_channel`
//! - `on_configure_chain_input`       / `on_configure_chain_output`
//!
//! The select_*_device callbacks rebuild the channel list against fresh device
//! enumeration; toggle_*_channel mutates the `ChainDraft` and re-publishes the
//! row. The configure_chain_input/output paths populate the IO groups model
//! from the chain's first Input / last Output and route the user to either
//! the inline fullscreen view or the dedicated child window.

use std::cell::{Cell, RefCell};
use std::rc::Rc;

use slint::{ComponentHandle, Model, ModelRc, SharedString, Timer, VecModel};

use project::chain::{ChainInputMode, ChainOutputMode};

use crate::audio_devices::{
    build_input_channel_items, build_output_channel_items, refresh_input_devices,
    refresh_output_devices, replace_channel_options, selected_device_index,
};
use crate::chain_editor::chain_draft_from_chain;
use crate::helpers::{set_status_error, show_child_window};
use crate::io_groups::{apply_chain_io_groups, build_io_group_items};
use crate::state::{ChainDraft, InputGroupDraft, OutputGroupDraft, ProjectSession};
use crate::{
    AppWindow, ChainEditorWindow, ChainInputGroupsWindow, ChainInputWindow,
    ChainOutputGroupsWindow, ChainOutputWindow, ChannelOptionItem,
};

pub(crate) struct ChainIoMainCtx {
    pub chain_draft: Rc<RefCell<Option<ChainDraft>>>,
    pub project_session: Rc<RefCell<Option<ProjectSession>>>,
    pub chain_editor_window: Rc<RefCell<Option<ChainEditorWindow>>>,
    pub chain_input_device_options: Rc<VecModel<SharedString>>,
    pub chain_output_device_options: Rc<VecModel<SharedString>>,
    pub chain_input_channels: Rc<VecModel<ChannelOptionItem>>,
    pub chain_output_channels: Rc<VecModel<ChannelOptionItem>>,
    pub inline_io_groups_is_input: Rc<Cell<bool>>,
    pub toast_timer: Rc<Timer>,
}

pub(crate) fn wire(
    window: &AppWindow,
    chain_input_window: &ChainInputWindow,
    chain_output_window: &ChainOutputWindow,
    chain_input_groups_window: &ChainInputGroupsWindow,
    chain_output_groups_window: &ChainOutputGroupsWindow,
    ctx: ChainIoMainCtx,
) {
    let ChainIoMainCtx {
        chain_draft,
        project_session,
        chain_editor_window,
        chain_input_device_options,
        chain_output_device_options,
        chain_input_channels,
        chain_output_channels,
        inline_io_groups_is_input,
        toast_timer,
    } = ctx;

    {
        let weak_window = window.as_weak();
        let weak_input_window = chain_input_window.as_weak();
        let chain_editor_window_ref = chain_editor_window.clone();
        let chain_draft = chain_draft.clone();
        let project_session = project_session.clone();
        let chain_input_device_options = chain_input_device_options.clone();
        let chain_output_device_options = chain_output_device_options.clone();
        let chain_input_channels = chain_input_channels.clone();
        window.on_select_chain_input_device(move |index| {
            let Some(window) = weak_window.upgrade() else {
                return;
            };
            let fresh_input = refresh_input_devices(&chain_input_device_options);
            let fresh_output = refresh_output_devices(&chain_output_device_options);
            let mut draft_borrow = chain_draft.borrow_mut();
            let Some(draft) = draft_borrow.as_mut() else {
                return;
            };
            let Some(device) = fresh_input.get(index as usize) else {
                return;
            };
            let Some(gi) = draft.editing_input_index else {
                return;
            };
            // Mutate the group first
            {
                let Some(input_group) = draft.inputs.get_mut(gi) else {
                    return;
                };
                input_group.device_id = Some(device.id.clone());
                input_group.channels.clear();
            }
            // Now use immutable references
            if project_session.borrow().is_some() {
                if let Some(input_group) = draft.inputs.get(gi) {
                    let channel_items = build_input_channel_items(input_group, &fresh_input);
                    replace_channel_options(&chain_input_channels, channel_items.clone());
                    // Fullscreen: sync channels to inline endpoint editor.
                    // Re-assign the same shared Rc<VecModel> so toggle handlers
                    // continue to operate on the same model the UI is bound to.
                    if window.get_fullscreen() {
                        window.set_chain_io_channels(ModelRc::from(chain_input_channels.clone()));
                    }
                }
                if let Some(chain_window) = chain_editor_window_ref.borrow().as_ref() {
                    apply_chain_io_groups(
                        &window,
                        chain_window,
                        draft,
                        &fresh_input,
                        &fresh_output,
                    );
                }
            }
            let selected_index = selected_device_index(
                &fresh_input,
                draft.inputs.get(gi).and_then(|ig| ig.device_id.as_deref()),
            );
            window.set_selected_chain_input_device_index(selected_index);
            if window.get_fullscreen() {
                window.set_chain_io_selected_device_index(selected_index);
            }
            if let Some(input_window) = weak_input_window.upgrade() {
                input_window.set_selected_device_index(selected_index);
            }
            if let Some(chain_window) = chain_editor_window_ref.borrow().as_ref() {
                if chain_window.get_show_input_editor() {
                    chain_window.set_input_selected_device_index(selected_index);
                }
            }
        });
    }
    {
        let weak_window = window.as_weak();
        let weak_output_window = chain_output_window.as_weak();
        let chain_editor_window_ref = chain_editor_window.clone();
        let chain_draft = chain_draft.clone();
        let project_session = project_session.clone();
        let chain_input_device_options = chain_input_device_options.clone();
        let chain_output_device_options = chain_output_device_options.clone();
        let chain_output_channels = chain_output_channels.clone();
        window.on_select_chain_output_device(move |index| {
            let Some(window) = weak_window.upgrade() else {
                return;
            };
            let fresh_input = refresh_input_devices(&chain_input_device_options);
            let fresh_output = refresh_output_devices(&chain_output_device_options);
            let mut draft_borrow = chain_draft.borrow_mut();
            let Some(draft) = draft_borrow.as_mut() else {
                return;
            };
            let Some(device) = fresh_output.get(index as usize) else {
                return;
            };
            let Some(gi) = draft.editing_output_index else {
                return;
            };
            {
                let Some(output_group) = draft.outputs.get_mut(gi) else {
                    return;
                };
                output_group.device_id = Some(device.id.clone());
                output_group.channels.clear();
            }
            if project_session.borrow().as_ref().is_some() {
                if let Some(output_group) = draft.outputs.get(gi) {
                    let channel_items = build_output_channel_items(output_group, &fresh_output);
                    replace_channel_options(&chain_output_channels, channel_items.clone());
                    if window.get_fullscreen() {
                        window.set_chain_io_channels(ModelRc::from(chain_output_channels.clone()));
                    }
                }
                if let Some(chain_window) = chain_editor_window_ref.borrow().as_ref() {
                    apply_chain_io_groups(
                        &window,
                        chain_window,
                        draft,
                        &fresh_input,
                        &fresh_output,
                    );
                }
            }
            let selected_index = selected_device_index(
                &fresh_output,
                draft.outputs.get(gi).and_then(|og| og.device_id.as_deref()),
            );
            window.set_selected_chain_output_device_index(selected_index);
            if window.get_fullscreen() {
                window.set_chain_io_selected_device_index(selected_index);
            }
            if let Some(output_window) = weak_output_window.upgrade() {
                output_window.set_selected_device_index(selected_index);
            }
            if let Some(chain_window) = chain_editor_window_ref.borrow().as_ref() {
                if chain_window.get_show_output_editor() {
                    chain_window.set_output_selected_device_index(selected_index);
                }
            }
        });
    }
    {
        let chain_draft = chain_draft.clone();
        let chain_input_channels = chain_input_channels.clone();
        let weak_window = window.as_weak();
        let toast_timer = toast_timer.clone();
        window.on_toggle_chain_input_channel(move |index, selected| {
            let Some(window) = weak_window.upgrade() else {
                return;
            };
            let mut draft_borrow = chain_draft.borrow_mut();
            let Some(draft) = draft_borrow.as_mut() else {
                return;
            };
            let channel = index as usize;
            let Some(option) = chain_input_channels.row_data(index as usize) else {
                return;
            };
            if selected && !option.available && !option.selected {
                set_status_error(
                    &window,
                    &toast_timer,
                    &rust_i18n::t!("error-channel-in-use-by-other"),
                );
                return;
            }
            let Some(gi) = draft.editing_input_index else {
                return;
            };
            {
                let Some(input_group) = draft.inputs.get_mut(gi) else {
                    return;
                };
                if selected {
                    if !input_group.channels.contains(&channel) {
                        input_group.channels.push(channel);
                        input_group.channels.sort_unstable();
                    }
                } else {
                    input_group.channels.retain(|current| *current != channel);
                }
            }
            if let Some(mut row) = chain_input_channels.row_data(index as usize) {
                row.selected = selected;
                chain_input_channels.set_row_data(index as usize, row);
            }
        });
    }
    {
        let chain_draft = chain_draft.clone();
        let chain_output_channels = chain_output_channels.clone();
        window.on_toggle_chain_output_channel(move |index, selected| {
            let mut draft_borrow = chain_draft.borrow_mut();
            let Some(draft) = draft_borrow.as_mut() else {
                return;
            };
            let channel = index as usize;
            let Some(gi) = draft.editing_output_index else {
                return;
            };
            {
                let Some(output_group) = draft.outputs.get_mut(gi) else {
                    return;
                };
                if selected {
                    if !output_group.channels.contains(&channel) {
                        output_group.channels.push(channel);
                        output_group.channels.sort_unstable();
                    }
                } else {
                    output_group.channels.retain(|current| *current != channel);
                }
            }
            if let Some(mut row) = chain_output_channels.row_data(index as usize) {
                row.selected = selected;
                chain_output_channels.set_row_data(index as usize, row);
            }
        });
    }
    {
        let weak_window = window.as_weak();
        let weak_groups_window = chain_input_groups_window.as_weak();
        let chain_draft = chain_draft.clone();
        let project_session = project_session.clone();
        let chain_input_device_options = chain_input_device_options.clone();
        let chain_output_device_options = chain_output_device_options.clone();
        let inline_io_groups_is_input = inline_io_groups_is_input.clone();
        window.on_configure_chain_input(move |index| {
            inline_io_groups_is_input.set(true);
            log::warn!("[UI] configure-chain-input clicked, chain_index={}", index);
            let Some(window) = weak_window.upgrade() else {
                log::warn!("[UI] configure-chain-input: window upgrade failed");
                return;
            };
            let Some(groups_window) = weak_groups_window.upgrade() else {
                log::warn!("[UI] configure-chain-input: groups_window upgrade failed");
                return;
            };
            let fresh_input = refresh_input_devices(&chain_input_device_options);
            let fresh_output = refresh_output_devices(&chain_output_device_options);
            let session_borrow = project_session.borrow();
            let Some(session) = session_borrow.as_ref() else {
                return;
            };
            let Some(chain) = session.project.chains.get(index as usize) else {
                return;
            };
            // Only show entries from the FIRST InputBlock (position 0 chip)
            let first_input = chain.first_input();
            let inputs: Vec<InputGroupDraft> = first_input
                .map(|ib| {
                    ib.entries
                        .iter()
                        .map(|e| InputGroupDraft {
                            device_id: if e.device_id.0.is_empty() {
                                None
                            } else {
                                Some(e.device_id.0.clone())
                            },
                            channels: e.channels.clone(),
                            mode: e.mode,
                        })
                        .collect()
                })
                .unwrap_or_else(|| {
                    vec![InputGroupDraft {
                        device_id: None,
                        channels: Vec::new(),
                        mode: ChainInputMode::Mono,
                    }]
                });
            let mut draft = chain_draft_from_chain(index as usize, chain);
            draft.inputs = inputs;
            let (input_items, _) = build_io_group_items(&draft, &fresh_input, &fresh_output);
            let groups_model = ModelRc::from(Rc::new(VecModel::from(input_items)));
            groups_window.set_groups(groups_model.clone());
            groups_window.set_status_message("".into());
            groups_window.set_show_block_controls(false);
            *chain_draft.borrow_mut() = Some(draft);
            if window.get_fullscreen() {
                window.set_chain_io_groups_title(
                    rust_i18n::t!("title-section-inputs").as_ref().into(),
                );
                window.set_chain_io_groups_add_label(
                    rust_i18n::t!("btn-add-input-row").as_ref().into(),
                );
                window.set_chain_io_groups_items(groups_model);
                window.set_chain_io_groups_status("".into());
                window.set_chain_io_groups_show_block_controls(false);
                window.set_show_chain_io_groups(true);
            } else {
                show_child_window(window.window(), groups_window.window());
            }
        });
    }
    {
        let weak_window = window.as_weak();
        let weak_groups_window = chain_output_groups_window.as_weak();
        let chain_draft = chain_draft.clone();
        let project_session = project_session.clone();
        let chain_input_device_options = chain_input_device_options.clone();
        let chain_output_device_options = chain_output_device_options.clone();
        let inline_io_groups_is_input = inline_io_groups_is_input.clone();
        window.on_configure_chain_output(move |index| {
            inline_io_groups_is_input.set(false);
            log::warn!("[UI] configure-chain-output clicked, chain_index={}", index);
            let Some(window) = weak_window.upgrade() else {
                log::warn!("[UI] configure-chain-output: window upgrade failed");
                return;
            };
            let Some(groups_window) = weak_groups_window.upgrade() else {
                log::warn!("[UI] configure-chain-output: groups_window upgrade failed");
                return;
            };
            let fresh_input = refresh_input_devices(&chain_input_device_options);
            let fresh_output = refresh_output_devices(&chain_output_device_options);
            let session_borrow = project_session.borrow();
            let Some(session) = session_borrow.as_ref() else {
                return;
            };
            let Some(chain) = session.project.chains.get(index as usize) else {
                return;
            };
            // Only show entries from the LAST OutputBlock (fixed output chip)
            let last_output = chain.last_output();
            let outputs: Vec<OutputGroupDraft> = last_output
                .map(|ob| {
                    ob.entries
                        .iter()
                        .map(|e| OutputGroupDraft {
                            device_id: if e.device_id.0.is_empty() {
                                None
                            } else {
                                Some(e.device_id.0.clone())
                            },
                            channels: e.channels.clone(),
                            mode: e.mode,
                        })
                        .collect()
                })
                .unwrap_or_else(|| {
                    vec![OutputGroupDraft {
                        device_id: None,
                        channels: Vec::new(),
                        mode: ChainOutputMode::Stereo,
                    }]
                });
            let mut draft = chain_draft_from_chain(index as usize, chain);
            draft.outputs = outputs;
            let (_, output_items) = build_io_group_items(&draft, &fresh_input, &fresh_output);
            let groups_model = ModelRc::from(Rc::new(VecModel::from(output_items)));
            groups_window.set_groups(groups_model.clone());
            groups_window.set_status_message("".into());
            groups_window.set_show_block_controls(false);
            *chain_draft.borrow_mut() = Some(draft);
            if window.get_fullscreen() {
                window.set_chain_io_groups_title(
                    rust_i18n::t!("title-section-outputs").as_ref().into(),
                );
                window.set_chain_io_groups_add_label(
                    rust_i18n::t!("btn-add-output-row").as_ref().into(),
                );
                window.set_chain_io_groups_items(groups_model);
                window.set_chain_io_groups_status("".into());
                window.set_chain_io_groups_show_block_controls(false);
                window.set_show_chain_io_groups(true);
            } else {
                show_child_window(window.window(), groups_window.window());
            }
        });
    }
}
