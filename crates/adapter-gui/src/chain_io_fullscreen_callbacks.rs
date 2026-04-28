//! Fullscreen-mode I/O editor + groups callbacks on the main window.
//!
//! In windowed mode, the I/O endpoint editor and the groups list each open
//! in their own child windows (`ChainInputWindow`, `ChainOutputWindow`,
//! `ChainInputGroupsWindow`, `ChainOutputGroupsWindow`). In fullscreen mode
//! those child windows can't render, so the same actions are exposed on the
//! main `AppWindow` and routed here. Each callback decides which target to
//! invoke based on three signals:
//!
//! * `inline_io_groups_is_input` — input vs output side
//! * `AppWindow.show_chain_io_groups` — coming from the groups list flow
//! * the active `ChainEditorWindow` (if any) showing input/output editor
//!
//! There are two groups of callbacks:
//!
//! 1. **endpoint editor** (`on_chain_io_select_device`,
//!    `on_chain_io_toggle_channel`, `on_chain_io_select_mode`,
//!    `on_chain_io_save`, `on_chain_io_cancel`) — drive the device + channel
//!    + mode picker shown for one input or output endpoint at a time.
//! 2. **groups list** (`on_chain_io_groups_edit/remove/add/save/cancel/
//!    toggle_enabled/delete_block`) — operate on the list of input/output
//!    groups for a chain. Only `groups_edit` does heavy work in fullscreen
//!    (seeds the editor draft directly instead of opening a child window);
//!    the rest delegate to the corresponding child-window invocations and
//!    sync the resulting state back to the main window.
//!
//! Wired once from `run_desktop_app`.

use std::cell::{Cell, RefCell};
use std::rc::Rc;

use slint::{ComponentHandle, ModelRc, VecModel};

use crate::audio_devices::{
    build_input_channel_items, build_output_channel_items, refresh_input_devices,
    refresh_output_devices, replace_channel_options, selected_device_index,
};
use crate::chain_editor::{input_mode_to_index, output_mode_to_index};
use crate::state::{ChainDraft, ProjectSession};
use crate::{
    AppWindow, ChainEditorWindow, ChainInputGroupsWindow, ChainInputWindow,
    ChainOutputGroupsWindow, ChainOutputWindow, ChannelOptionItem,
};

pub(crate) struct ChainIoFullscreenCallbacksCtx {
    pub chain_editor_window: Rc<RefCell<Option<ChainEditorWindow>>>,
    pub inline_io_groups_is_input: Rc<Cell<bool>>,
    pub chain_draft: Rc<RefCell<Option<ChainDraft>>>,
    pub project_session: Rc<RefCell<Option<ProjectSession>>>,
    pub chain_input_device_options: Rc<VecModel<slint::SharedString>>,
    pub chain_output_device_options: Rc<VecModel<slint::SharedString>>,
    pub chain_input_channels: Rc<VecModel<ChannelOptionItem>>,
    pub chain_output_channels: Rc<VecModel<ChannelOptionItem>>,
}

pub(crate) fn wire(
    window: &AppWindow,
    chain_input_window: &ChainInputWindow,
    chain_output_window: &ChainOutputWindow,
    chain_input_groups_window: &ChainInputGroupsWindow,
    chain_output_groups_window: &ChainOutputGroupsWindow,
    ctx: ChainIoFullscreenCallbacksCtx,
) {
    let ChainIoFullscreenCallbacksCtx {
        chain_editor_window,
        inline_io_groups_is_input,
        chain_draft,
        project_session,
        chain_input_device_options,
        chain_output_device_options,
        chain_input_channels,
        chain_output_channels,
    } = ctx;

    // on_chain_io_select_device
    {
        let chain_editor_window = chain_editor_window.clone();
        let weak_window = window.as_weak();
        let weak_input_window = chain_input_window.as_weak();
        let weak_output_window = chain_output_window.as_weak();
        let inline_flag = inline_io_groups_is_input.clone();
        window.on_chain_io_select_device(move |index| {
            let from_groups = weak_window.upgrade().map_or(false, |w| w.get_show_chain_io_groups());
            if from_groups {
                if inline_flag.get() {
                    if let Some(iw) = weak_input_window.upgrade() { iw.invoke_select_device(index); }
                } else {
                    if let Some(ow) = weak_output_window.upgrade() { ow.invoke_select_device(index); }
                }
            } else {
                if let Some(cew) = chain_editor_window.borrow().as_ref() {
                    if cew.get_show_input_editor() {
                        cew.invoke_input_select_device(index);
                    } else if cew.get_show_output_editor() {
                        cew.invoke_output_select_device(index);
                    }
                }
            }
        });
    }

    // on_chain_io_toggle_channel
    {
        let chain_editor_window = chain_editor_window.clone();
        let weak_window = window.as_weak();
        let inline_flag = inline_io_groups_is_input.clone();
        window.on_chain_io_toggle_channel(move |index, selected| {
            let Some(w) = weak_window.upgrade() else { return; };
            let from_groups = w.get_show_chain_io_groups();
            if from_groups {
                // Delegate to AppWindow's toggle handler which updates the
                // shared chain_input_channels / chain_output_channels VecModel.
                // Since on_chain_io_groups_edit already set chain-io-channels
                // to point at the same shared VecModel, changes are reflected
                // automatically — no sync needed.
                if inline_flag.get() {
                    w.invoke_toggle_chain_input_channel(index, selected);
                } else {
                    w.invoke_toggle_chain_output_channel(index, selected);
                }
            } else {
                if let Some(cew) = chain_editor_window.borrow().as_ref() {
                    if cew.get_show_input_editor() {
                        cew.invoke_input_toggle_channel(index, selected);
                    } else if cew.get_show_output_editor() {
                        cew.invoke_output_toggle_channel(index, selected);
                    }
                }
            }
        });
    }

    // on_chain_io_select_mode
    {
        let chain_editor_window = chain_editor_window.clone();
        let weak_window = window.as_weak();
        let weak_input_window = chain_input_window.as_weak();
        let weak_output_window = chain_output_window.as_weak();
        let inline_flag = inline_io_groups_is_input.clone();
        window.on_chain_io_select_mode(move |index| {
            let from_groups = weak_window.upgrade().map_or(false, |w| w.get_show_chain_io_groups());
            if from_groups {
                if inline_flag.get() {
                    if let Some(iw) = weak_input_window.upgrade() { iw.invoke_select_input_mode(index); }
                } else {
                    if let Some(ow) = weak_output_window.upgrade() { ow.invoke_select_output_mode(index); }
                }
            } else {
                if let Some(cew) = chain_editor_window.borrow().as_ref() {
                    if cew.get_show_input_editor() {
                        cew.invoke_input_select_mode(index);
                    } else if cew.get_show_output_editor() {
                        cew.invoke_output_select_mode(index);
                    }
                }
            }
        });
    }

    // on_chain_io_save
    {
        let chain_editor_window = chain_editor_window.clone();
        let weak_window = window.as_weak();
        let weak_input_window = chain_input_window.as_weak();
        let weak_output_window = chain_output_window.as_weak();
        let inline_flag = inline_io_groups_is_input.clone();
        let weak_input_groups = chain_input_groups_window.as_weak();
        let weak_output_groups = chain_output_groups_window.as_weak();
        window.on_chain_io_save(move || {
            let Some(w) = weak_window.upgrade() else { return; };
            if w.get_show_chain_io_groups() {
                // Came from groups flow — delegate to ChainInputWindow/ChainOutputWindow
                if inline_flag.get() {
                    if let Some(iw) = weak_input_window.upgrade() {
                        iw.invoke_save();
                    }
                    // Sync groups back after save
                    if let Some(gw) = weak_input_groups.upgrade() {
                        w.set_chain_io_groups_items(gw.get_groups());
                    }
                } else {
                    if let Some(ow) = weak_output_window.upgrade() {
                        ow.invoke_save();
                    }
                    if let Some(gw) = weak_output_groups.upgrade() {
                        w.set_chain_io_groups_items(gw.get_groups());
                    }
                }
            } else {
                // Came from chain editor flow
                if let Some(cew) = chain_editor_window.borrow().as_ref() {
                    if cew.get_show_input_editor() {
                        cew.invoke_input_save();
                    } else if cew.get_show_output_editor() {
                        cew.invoke_output_save();
                    }
                }
            }
            w.set_show_chain_io_editor(false);
        });
    }

    // on_chain_io_cancel
    {
        let chain_editor_window = chain_editor_window.clone();
        let weak_window = window.as_weak();
        let weak_input_window = chain_input_window.as_weak();
        let weak_output_window = chain_output_window.as_weak();
        let inline_flag = inline_io_groups_is_input.clone();
        window.on_chain_io_cancel(move || {
            let Some(w) = weak_window.upgrade() else { return; };
            if w.get_show_chain_io_groups() {
                // Came from groups flow — delegate cancel
                if inline_flag.get() {
                    if let Some(iw) = weak_input_window.upgrade() {
                        iw.invoke_cancel();
                    }
                } else {
                    if let Some(ow) = weak_output_window.upgrade() {
                        ow.invoke_cancel();
                    }
                }
            } else {
                // Came from chain editor flow
                if let Some(cew) = chain_editor_window.borrow().as_ref() {
                    if cew.get_show_input_editor() {
                        cew.invoke_input_cancel();
                    } else if cew.get_show_output_editor() {
                        cew.invoke_output_cancel();
                    }
                }
            }
            w.set_show_chain_io_editor(false);
        });
    }

    // on_chain_io_groups_edit
    {
        let inline_flag = inline_io_groups_is_input.clone();
        let weak_window = window.as_weak();
        let chain_input_device_options = chain_input_device_options.clone();
        let chain_output_device_options = chain_output_device_options.clone();
        let chain_draft = chain_draft.clone();
        let project_session = project_session.clone();
        let chain_input_channels = chain_input_channels.clone();
        let chain_output_channels = chain_output_channels.clone();
        window.on_chain_io_groups_edit(move |group_index| {
            let Some(window) = weak_window.upgrade() else { return; };
            if inline_flag.get() {
                // In fullscreen, we set up draft state directly instead of
                // calling invoke_edit_group() which would open a child window.
                let fresh_input = refresh_input_devices(&chain_input_device_options);
                let mut draft_borrow = chain_draft.borrow_mut();
                if let Some(draft) = draft_borrow.as_mut() {
                    let gi = group_index as usize;
                    draft.editing_input_index = Some(gi);
                    if let Some(input_group) = draft.inputs.get(gi) {
                        let session_borrow = project_session.borrow();
                        if let Some(session) = session_borrow.as_ref() {
                            let dev_idx = selected_device_index(
                                &fresh_input,
                                input_group.device_id.as_deref(),
                            );
                            let mode_idx = input_mode_to_index(input_group.mode);
                            let channel_items = build_input_channel_items(input_group, draft, &session.project, &fresh_input);
                            let labels: Vec<slint::SharedString> = vec!["Mono".into(), "Stereo".into(), "Dual Mono".into()];
                            let device_strings: Vec<slint::SharedString> = fresh_input.iter().map(|d| slint::SharedString::from(d.name.as_str())).collect();
                            // Sync shared VecModel so toggle_chain_input_channel works
                            log::info!(
                                "[groups_edit INPUT] gi={} dev_idx={} device_id={:?} fresh_devices={} channel_items={} mode_idx={}",
                                gi, dev_idx, input_group.device_id, fresh_input.len(), channel_items.len(), mode_idx
                            );
                            for (ci, ch) in channel_items.iter().enumerate() {
                                log::info!("[groups_edit INPUT]   ch[{}] label='{}' selected={} available={}", ci, ch.label, ch.selected, ch.available);
                            }
                            replace_channel_options(&chain_input_channels, channel_items.clone());
                            window.set_chain_io_editor_title("Entrada".into());
                            window.set_chain_io_device_options(ModelRc::from(Rc::new(VecModel::from(device_strings.clone()))));
                            log::info!("[groups_edit INPUT] device_strings={:?}", device_strings);
                            window.set_chain_io_selected_device_index(dev_idx);
                            window.set_chain_io_channels(ModelRc::from(chain_input_channels.clone()));
                            window.set_chain_io_editor_status("".into());
                            window.set_chain_io_show_mode_selector(true);
                            window.set_chain_io_mode_labels(ModelRc::from(Rc::new(VecModel::from(labels))));
                            window.set_chain_io_selected_mode_index(mode_idx);
                            window.set_show_chain_io_editor(true);
                        }
                    }
                }
            } else {
                // In fullscreen, we set up draft state directly instead of
                // calling invoke_edit_group() which would open a child window.
                let fresh_output = refresh_output_devices(&chain_output_device_options);
                let mut draft_borrow = chain_draft.borrow_mut();
                if let Some(draft) = draft_borrow.as_mut() {
                    let gi = group_index as usize;
                    draft.editing_output_index = Some(gi);
                    if let Some(output_group) = draft.outputs.get(gi) {
                        let dev_idx = selected_device_index(
                            &fresh_output,
                            output_group.device_id.as_deref(),
                        );
                        let mode_idx = output_mode_to_index(output_group.mode);
                        let channel_items = build_output_channel_items(output_group, &fresh_output);
                        let labels: Vec<slint::SharedString> = vec!["Mono".into(), "Stereo".into()];
                        let device_strings: Vec<slint::SharedString> = fresh_output.iter().map(|d| slint::SharedString::from(d.name.as_str())).collect();
                        // Sync shared VecModel so toggle_chain_output_channel works
                        log::info!(
                            "[groups_edit OUTPUT] gi={} dev_idx={} device_id={:?} fresh_devices={} channel_items={} mode_idx={}",
                            gi, dev_idx, output_group.device_id, fresh_output.len(), channel_items.len(), mode_idx
                        );
                        for (ci, ch) in channel_items.iter().enumerate() {
                            log::info!("[groups_edit OUTPUT]   ch[{}] label='{}' selected={} available={}", ci, ch.label, ch.selected, ch.available);
                        }
                        replace_channel_options(&chain_output_channels, channel_items.clone());
                        window.set_chain_io_editor_title("Saída".into());
                        window.set_chain_io_device_options(ModelRc::from(Rc::new(VecModel::from(device_strings.clone()))));
                        log::info!("[groups_edit OUTPUT] device_strings={:?}", device_strings);
                        window.set_chain_io_selected_device_index(dev_idx);
                        window.set_chain_io_channels(ModelRc::from(chain_output_channels.clone()));
                        window.set_chain_io_editor_status("".into());
                        window.set_chain_io_show_mode_selector(true);
                        window.set_chain_io_mode_labels(ModelRc::from(Rc::new(VecModel::from(labels))));
                        window.set_chain_io_selected_mode_index(mode_idx);
                        window.set_show_chain_io_editor(true);
                    }
                }
            }
        });
    }

    // on_chain_io_groups_remove
    {
        let weak_input_groups = chain_input_groups_window.as_weak();
        let weak_output_groups = chain_output_groups_window.as_weak();
        let inline_flag = inline_io_groups_is_input.clone();
        let weak_window = window.as_weak();
        window.on_chain_io_groups_remove(move |group_index| {
            if inline_flag.get() {
                if let Some(gw) = weak_input_groups.upgrade() {
                    gw.invoke_remove_group(group_index);
                    // Sync updated groups back to AppWindow
                    if let Some(w) = weak_window.upgrade() {
                        w.set_chain_io_groups_items(gw.get_groups());
                    }
                }
            } else {
                if let Some(gw) = weak_output_groups.upgrade() {
                    gw.invoke_remove_group(group_index);
                    if let Some(w) = weak_window.upgrade() {
                        w.set_chain_io_groups_items(gw.get_groups());
                    }
                }
            }
        });
    }

    // on_chain_io_groups_add
    {
        let weak_input_groups = chain_input_groups_window.as_weak();
        let weak_output_groups = chain_output_groups_window.as_weak();
        let inline_flag = inline_io_groups_is_input.clone();
        let weak_window = window.as_weak();
        window.on_chain_io_groups_add(move || {
            if inline_flag.get() {
                if let Some(gw) = weak_input_groups.upgrade() {
                    gw.invoke_add_group();
                    if let Some(w) = weak_window.upgrade() {
                        w.set_chain_io_groups_items(gw.get_groups());
                    }
                }
            } else {
                if let Some(gw) = weak_output_groups.upgrade() {
                    gw.invoke_add_group();
                    if let Some(w) = weak_window.upgrade() {
                        w.set_chain_io_groups_items(gw.get_groups());
                    }
                }
            }
        });
    }

    // on_chain_io_groups_save
    {
        let weak_input_groups = chain_input_groups_window.as_weak();
        let weak_output_groups = chain_output_groups_window.as_weak();
        let inline_flag = inline_io_groups_is_input.clone();
        let weak_window = window.as_weak();
        window.on_chain_io_groups_save(move || {
            if inline_flag.get() {
                if let Some(gw) = weak_input_groups.upgrade() {
                    gw.invoke_save();
                }
            } else {
                if let Some(gw) = weak_output_groups.upgrade() {
                    gw.invoke_save();
                }
            }
            if let Some(w) = weak_window.upgrade() {
                w.set_show_chain_io_groups(false);
            }
        });
    }

    // on_chain_io_groups_cancel
    {
        let weak_input_groups = chain_input_groups_window.as_weak();
        let weak_output_groups = chain_output_groups_window.as_weak();
        let inline_flag = inline_io_groups_is_input.clone();
        let weak_window = window.as_weak();
        window.on_chain_io_groups_cancel(move || {
            if inline_flag.get() {
                if let Some(gw) = weak_input_groups.upgrade() {
                    gw.invoke_cancel();
                }
            } else {
                if let Some(gw) = weak_output_groups.upgrade() {
                    gw.invoke_cancel();
                }
            }
            if let Some(w) = weak_window.upgrade() {
                w.set_show_chain_io_groups(false);
            }
        });
    }

    // on_chain_io_groups_toggle_enabled
    {
        let weak_input_groups = chain_input_groups_window.as_weak();
        let weak_output_groups = chain_output_groups_window.as_weak();
        let inline_flag = inline_io_groups_is_input.clone();
        window.on_chain_io_groups_toggle_enabled(move || {
            if inline_flag.get() {
                if let Some(gw) = weak_input_groups.upgrade() {
                    gw.invoke_toggle_enabled();
                }
            } else {
                if let Some(gw) = weak_output_groups.upgrade() {
                    gw.invoke_toggle_enabled();
                }
            }
        });
    }

    // on_chain_io_groups_delete_block
    {
        let weak_input_groups = chain_input_groups_window.as_weak();
        let weak_output_groups = chain_output_groups_window.as_weak();
        let inline_flag = inline_io_groups_is_input.clone();
        let weak_window = window.as_weak();
        window.on_chain_io_groups_delete_block(move || {
            if inline_flag.get() {
                if let Some(gw) = weak_input_groups.upgrade() {
                    gw.invoke_delete_block();
                }
            } else {
                if let Some(gw) = weak_output_groups.upgrade() {
                    gw.invoke_delete_block();
                }
            }
            if let Some(w) = weak_window.upgrade() {
                w.set_show_chain_io_groups(false);
            }
        });
    }
}
