//! Chain metadata (name, instrument) and I/O group editing callbacks for
//! the per-instance `ChainEditorWindow`.
//!
//! Wires `on_update_chain_name`, `on_select_instrument`, `on_edit_input`,
//! `on_edit_output`, `on_add_input`, `on_add_output`, `on_remove_input`,
//! `on_remove_output`. The save / cancel of the whole chain and the inline
//! input / output endpoint editors live in sibling modules.

use std::cell::RefCell;
use std::rc::Rc;

use slint::{ComponentHandle, ModelRc, SharedString, VecModel};

use infra_cpal::AudioDeviceDescriptor;
use project::chain::{ChainInputMode, ChainOutputMode};

use crate::audio_devices::{refresh_input_devices, refresh_output_devices, selected_device_index};
use crate::chain_editor::{input_mode_to_index, instrument_index_to_string, output_mode_to_index};
use crate::io_groups::apply_chain_io_groups;
use crate::state::{ChainDraft, InputGroupDraft, OutputGroupDraft};
use crate::{AppWindow, ChainEditorWindow, ChannelOptionItem};

#[allow(clippy::too_many_arguments)]
pub(crate) fn wire(
    editor_window: &ChainEditorWindow,
    weak_window: slint::Weak<AppWindow>,
    chain_draft: Rc<RefCell<Option<ChainDraft>>>,
    input_chain_devices: Rc<RefCell<Vec<AudioDeviceDescriptor>>>,
    output_chain_devices: Rc<RefCell<Vec<AudioDeviceDescriptor>>>,
    chain_input_device_options: Rc<VecModel<SharedString>>,
    chain_output_device_options: Rc<VecModel<SharedString>>,
    chain_input_channels: Rc<VecModel<ChannelOptionItem>>,
    chain_output_channels: Rc<VecModel<ChannelOptionItem>>,
) {
    // on_update_chain_name
    {
        let weak_window = weak_window.clone();
        let weak_chain_window = editor_window.as_weak();
        let chain_draft = chain_draft.clone();
        editor_window.on_update_chain_name(move |value| {
            let Some(window) = weak_window.upgrade() else {
                return;
            };
            let Some(chain_window) = weak_chain_window.upgrade() else {
                return;
            };
            if let Some(draft) = chain_draft.borrow_mut().as_mut() {
                draft.name = value.to_string();
                window.set_chain_draft_name(value.clone());
                chain_window.set_chain_name(value);
            }
        });
    }
    // on_select_instrument
    {
        let chain_draft = chain_draft.clone();
        editor_window.on_select_instrument(move |index| {
            let instrument = instrument_index_to_string(index).to_string();
            log::debug!(
                "[select_instrument] index={}, instrument='{}'",
                index,
                instrument
            );
            if let Some(draft) = chain_draft.borrow_mut().as_mut() {
                draft.instrument = instrument;
                log::debug!(
                    "[select_instrument] draft updated to '{}'",
                    draft.instrument
                );
            } else {
                log::warn!("[select_instrument] no draft to update!");
            }
        });
    }
    // on_edit_input
    {
        let weak_window = weak_window.clone();
        let weak_chain_window = editor_window.as_weak();
        let chain_draft = chain_draft.clone();
        let chain_input_device_options = chain_input_device_options.clone();
        let chain_output_device_options = chain_output_device_options.clone();
        let chain_input_channels = chain_input_channels.clone();
        editor_window.on_edit_input(move |group_index| {
            let Some(window) = weak_window.upgrade() else {
                return;
            };
            let fresh_input = refresh_input_devices(&chain_input_device_options);
            let fresh_output = refresh_output_devices(&chain_output_device_options);
            let gi = group_index as usize;
            {
                let mut draft_borrow = chain_draft.borrow_mut();
                let Some(draft) = draft_borrow.as_mut() else {
                    return;
                };
                draft.editing_input_index = Some(gi);
            }
            let draft_borrow = chain_draft.borrow();
            let Some(draft) = draft_borrow.as_ref() else {
                return;
            };
            if let Some(chain_window) = weak_chain_window.upgrade() {
                apply_chain_io_groups(&window, &chain_window, draft, &fresh_input, &fresh_output);
                let device_opts = ModelRc::from(chain_input_device_options.clone());
                let channel_opts = ModelRc::from(chain_input_channels.clone());
                chain_window.set_input_device_options(device_opts.clone());
                chain_window.set_input_channels(channel_opts.clone());
                let mut dev_idx = -1i32;
                let mut mode_idx = 0i32;
                if let Some(input_group) = draft.inputs.get(gi) {
                    dev_idx = selected_device_index(&fresh_input, input_group.device_id.as_deref());
                    mode_idx = input_mode_to_index(input_group.mode);
                    chain_window.set_input_selected_device_index(dev_idx);
                    chain_window.set_input_mode_index(mode_idx);
                }
                chain_window.set_input_editor_status("".into());
                chain_window.set_show_input_editor(true);
                // Fullscreen: propagate to main window inline I/O editor
                if window.get_fullscreen() {
                    let labels: Vec<slint::SharedString> =
                        vec!["Mono".into(), "Stereo".into(), "Dual Mono".into()];
                    let device_strings: Vec<slint::SharedString> = fresh_input
                        .iter()
                        .map(|d| slint::SharedString::from(d.name.as_str()))
                        .collect();
                    window.set_chain_io_editor_title(
                        rust_i18n::t!("chain-io-input-title").as_ref().into(),
                    );
                    window.set_chain_io_device_options(ModelRc::from(Rc::new(VecModel::from(
                        device_strings,
                    ))));
                    window.set_chain_io_selected_device_index(dev_idx);
                    window.set_chain_io_channels(channel_opts);
                    window.set_chain_io_editor_status("".into());
                    window.set_chain_io_show_mode_selector(true);
                    window.set_chain_io_mode_labels(ModelRc::from(Rc::new(VecModel::from(labels))));
                    window.set_chain_io_selected_mode_index(mode_idx);
                    window.set_show_chain_io_editor(true);
                }
            }
        });
    }
    // on_edit_output
    {
        let weak_window = weak_window.clone();
        let weak_chain_window = editor_window.as_weak();
        let chain_draft = chain_draft.clone();
        let chain_input_device_options = chain_input_device_options.clone();
        let chain_output_device_options = chain_output_device_options.clone();
        let chain_output_channels = chain_output_channels.clone();
        editor_window.on_edit_output(move |group_index| {
            let Some(window) = weak_window.upgrade() else {
                return;
            };
            let fresh_input = refresh_input_devices(&chain_input_device_options);
            let fresh_output = refresh_output_devices(&chain_output_device_options);
            let gi = group_index as usize;
            {
                let mut draft_borrow = chain_draft.borrow_mut();
                let Some(draft) = draft_borrow.as_mut() else {
                    return;
                };
                draft.editing_output_index = Some(gi);
            }
            let draft_borrow = chain_draft.borrow();
            let Some(draft) = draft_borrow.as_ref() else {
                return;
            };
            if let Some(chain_window) = weak_chain_window.upgrade() {
                apply_chain_io_groups(&window, &chain_window, draft, &fresh_input, &fresh_output);
                let device_opts = ModelRc::from(chain_output_device_options.clone());
                let channel_opts = ModelRc::from(chain_output_channels.clone());
                chain_window.set_output_device_options(device_opts.clone());
                chain_window.set_output_channels(channel_opts.clone());
                let mut dev_idx = -1i32;
                let mut mode_idx = 0i32;
                if let Some(output_group) = draft.outputs.get(gi) {
                    dev_idx =
                        selected_device_index(&fresh_output, output_group.device_id.as_deref());
                    mode_idx = output_mode_to_index(output_group.mode);
                    chain_window.set_output_selected_device_index(dev_idx);
                    chain_window.set_output_mode_index(mode_idx);
                }
                chain_window.set_output_editor_status("".into());
                chain_window.set_show_output_editor(true);
                // Fullscreen: propagate to main window inline I/O editor
                if window.get_fullscreen() {
                    let labels: Vec<slint::SharedString> = vec!["Mono".into(), "Stereo".into()];
                    let device_strings: Vec<slint::SharedString> = fresh_output
                        .iter()
                        .map(|d| slint::SharedString::from(d.name.as_str()))
                        .collect();
                    window.set_chain_io_editor_title(
                        rust_i18n::t!("chain-io-output-title").as_ref().into(),
                    );
                    window.set_chain_io_device_options(ModelRc::from(Rc::new(VecModel::from(
                        device_strings,
                    ))));
                    window.set_chain_io_selected_device_index(dev_idx);
                    window.set_chain_io_channels(channel_opts);
                    window.set_chain_io_editor_status("".into());
                    window.set_chain_io_show_mode_selector(true);
                    window.set_chain_io_mode_labels(ModelRc::from(Rc::new(VecModel::from(labels))));
                    window.set_chain_io_selected_mode_index(mode_idx);
                    window.set_show_chain_io_editor(true);
                }
            }
        });
    }
    // on_add_input
    {
        let weak_window = weak_window.clone();
        let weak_chain_window = editor_window.as_weak();
        let chain_draft = chain_draft.clone();
        let chain_input_device_options = chain_input_device_options.clone();
        let chain_output_device_options = chain_output_device_options.clone();
        let chain_input_channels = chain_input_channels.clone();
        editor_window.on_add_input(move || {
            let Some(window) = weak_window.upgrade() else {
                return;
            };
            let fresh_input = refresh_input_devices(&chain_input_device_options);
            let fresh_output = refresh_output_devices(&chain_output_device_options);
            let new_idx = {
                let mut draft_borrow = chain_draft.borrow_mut();
                let Some(draft) = draft_borrow.as_mut() else {
                    return;
                };
                let idx = draft.inputs.len();
                draft.inputs.push(InputGroupDraft {
                    device_id: fresh_input.first().map(|d| d.id.clone()),
                    channels: Vec::new(),
                    mode: ChainInputMode::Mono,
                });
                draft.editing_input_index = Some(idx);
                draft.adding_new_input = true;
                if let Some(chain_window) = weak_chain_window.upgrade() {
                    apply_chain_io_groups(
                        &window,
                        &chain_window,
                        draft,
                        &fresh_input,
                        &fresh_output,
                    );
                }
                idx
            };
            let draft_borrow = chain_draft.borrow();
            let Some(draft) = draft_borrow.as_ref() else {
                return;
            };
            if let Some(chain_window) = weak_chain_window.upgrade() {
                chain_window
                    .set_input_device_options(ModelRc::from(chain_input_device_options.clone()));
                chain_window.set_input_channels(ModelRc::from(chain_input_channels.clone()));
                if let Some(input_group) = draft.inputs.get(new_idx) {
                    chain_window.set_input_selected_device_index(selected_device_index(
                        &fresh_input,
                        input_group.device_id.as_deref(),
                    ));
                    chain_window.set_input_mode_index(input_mode_to_index(input_group.mode));
                }
                chain_window.set_input_editor_status("".into());
                chain_window.set_show_input_editor(true);
            }
        });
    }
    // on_add_output
    {
        let weak_window = weak_window.clone();
        let weak_chain_window = editor_window.as_weak();
        let chain_draft = chain_draft.clone();
        let chain_input_device_options = chain_input_device_options.clone();
        let chain_output_device_options = chain_output_device_options.clone();
        let chain_output_channels = chain_output_channels.clone();
        editor_window.on_add_output(move || {
            let Some(window) = weak_window.upgrade() else {
                return;
            };
            let fresh_input = refresh_input_devices(&chain_input_device_options);
            let fresh_output = refresh_output_devices(&chain_output_device_options);
            let new_idx = {
                let mut draft_borrow = chain_draft.borrow_mut();
                let Some(draft) = draft_borrow.as_mut() else {
                    return;
                };
                let idx = draft.outputs.len();
                draft.outputs.push(OutputGroupDraft {
                    device_id: fresh_output.first().map(|d| d.id.clone()),
                    channels: Vec::new(),
                    mode: ChainOutputMode::Stereo,
                });
                draft.editing_output_index = Some(idx);
                draft.adding_new_output = true;
                if let Some(chain_window) = weak_chain_window.upgrade() {
                    apply_chain_io_groups(
                        &window,
                        &chain_window,
                        draft,
                        &fresh_input,
                        &fresh_output,
                    );
                }
                idx
            };
            let draft_borrow = chain_draft.borrow();
            let Some(draft) = draft_borrow.as_ref() else {
                return;
            };
            if let Some(chain_window) = weak_chain_window.upgrade() {
                chain_window
                    .set_output_device_options(ModelRc::from(chain_output_device_options.clone()));
                chain_window.set_output_channels(ModelRc::from(chain_output_channels.clone()));
                if let Some(output_group) = draft.outputs.get(new_idx) {
                    chain_window.set_output_selected_device_index(selected_device_index(
                        &fresh_output,
                        output_group.device_id.as_deref(),
                    ));
                    chain_window.set_output_mode_index(output_mode_to_index(output_group.mode));
                }
                chain_window.set_output_editor_status("".into());
                chain_window.set_show_output_editor(true);
            }
        });
    }
    // on_remove_input
    {
        let weak_window = weak_window.clone();
        let weak_chain_window = editor_window.as_weak();
        let chain_draft = chain_draft.clone();
        let input_chain_devices = input_chain_devices.clone();
        let output_chain_devices = output_chain_devices.clone();
        editor_window.on_remove_input(move |group_index| {
            let Some(window) = weak_window.upgrade() else {
                return;
            };
            let Some(chain_window) = weak_chain_window.upgrade() else {
                return;
            };
            let mut draft_borrow = chain_draft.borrow_mut();
            let Some(draft) = draft_borrow.as_mut() else {
                return;
            };
            // Fixed block (chip In/Out): must keep at least one entry
            if draft.editing_io_block_index.is_none() && draft.inputs.len() <= 1 {
                return;
            }
            let gi = group_index as usize;
            if gi < draft.inputs.len() {
                draft.inputs.remove(gi);
                // Reset editing index if it was pointing to the removed group
                if draft.editing_input_index == Some(gi) {
                    draft.editing_input_index = None;
                } else if let Some(idx) = draft.editing_input_index {
                    if idx > gi {
                        draft.editing_input_index = Some(idx - 1);
                    }
                }
            }
            apply_chain_io_groups(
                &window,
                &chain_window,
                draft,
                &*input_chain_devices.borrow(),
                &*output_chain_devices.borrow(),
            );
        });
    }
    // on_remove_output
    {
        let weak_window = weak_window.clone();
        let weak_chain_window = editor_window.as_weak();
        let chain_draft = chain_draft.clone();
        let input_chain_devices = input_chain_devices.clone();
        let output_chain_devices = output_chain_devices.clone();
        editor_window.on_remove_output(move |group_index| {
            let Some(window) = weak_window.upgrade() else {
                return;
            };
            let Some(chain_window) = weak_chain_window.upgrade() else {
                return;
            };
            let mut draft_borrow = chain_draft.borrow_mut();
            let Some(draft) = draft_borrow.as_mut() else {
                return;
            };
            // Fixed block (chip In/Out): must keep at least one entry
            if draft.editing_io_block_index.is_none() && draft.outputs.len() <= 1 {
                return;
            }
            let gi = group_index as usize;
            if gi < draft.outputs.len() {
                draft.outputs.remove(gi);
                if draft.editing_output_index == Some(gi) {
                    draft.editing_output_index = None;
                } else if let Some(idx) = draft.editing_output_index {
                    if idx > gi {
                        draft.editing_output_index = Some(idx - 1);
                    }
                }
            }
            apply_chain_io_groups(
                &window,
                &chain_window,
                draft,
                &*input_chain_devices.borrow(),
                &*output_chain_devices.borrow(),
            );
        });
    }
}
