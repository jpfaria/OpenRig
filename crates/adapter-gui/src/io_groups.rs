use crate::audio_devices::{
    build_input_channel_items, build_output_channel_items, replace_channel_options,
    selected_device_index,
};
use crate::chain_editor::{endpoint_summary, input_mode_to_index, output_mode_to_index};
use crate::state::{ChainDraft, InputGroupDraft, OutputGroupDraft};
use crate::ChannelOptionItem;
use crate::{AppWindow, ChainEditorWindow, ChainInputWindow, ChainOutputWindow, IoGroupItem};
use infra_cpal::AudioDeviceDescriptor;
use slint::{ModelRc, VecModel};
use std::rc::Rc;

pub(crate) fn build_io_group_items(
    draft: &ChainDraft,
    input_devices: &[AudioDeviceDescriptor],
    output_devices: &[AudioDeviceDescriptor],
) -> (Vec<IoGroupItem>, Vec<IoGroupItem>) {
    let input_items: Vec<IoGroupItem> = draft
        .inputs
        .iter()
        .map(|input| {
            let summary =
                endpoint_summary(input.device_id.as_deref(), &input.channels, input_devices);
            IoGroupItem {
                summary: summary.into(),
            }
        })
        .collect();
    let output_items: Vec<IoGroupItem> = draft
        .outputs
        .iter()
        .map(|output| {
            let summary = endpoint_summary(
                output.device_id.as_deref(),
                &output.channels,
                output_devices,
            );
            IoGroupItem {
                summary: summary.into(),
            }
        })
        .collect();
    (input_items, output_items)
}

pub(crate) fn apply_chain_io_groups(
    window: &AppWindow,
    chain_editor_window: &ChainEditorWindow,
    draft: &ChainDraft,
    input_devices: &[AudioDeviceDescriptor],
    output_devices: &[AudioDeviceDescriptor],
) {
    let (input_items, output_items) = build_io_group_items(draft, input_devices, output_devices);
    // Update main window summaries (first input/output for legacy compat)
    let input_summary = draft
        .inputs
        .first()
        .map(|i| endpoint_summary(i.device_id.as_deref(), &i.channels, input_devices))
        .unwrap_or_default();
    let output_summary = draft
        .outputs
        .first()
        .map(|o| endpoint_summary(o.device_id.as_deref(), &o.channels, output_devices))
        .unwrap_or_default();
    window.set_chain_input_summary(input_summary.into());
    window.set_chain_output_summary(output_summary.into());
    chain_editor_window.set_input_groups(ModelRc::from(Rc::new(VecModel::from(input_items))));
    chain_editor_window.set_output_groups(ModelRc::from(Rc::new(VecModel::from(output_items))));
}

pub(crate) fn apply_chain_input_window_state(
    input_window: &ChainInputWindow,
    input_group: &InputGroupDraft,
    input_devices: &[AudioDeviceDescriptor],
    channel_model: &Rc<VecModel<ChannelOptionItem>>,
) {
    replace_channel_options(
        channel_model,
        build_input_channel_items(input_group, input_devices),
    );
    input_window.set_selected_device_index(selected_device_index(
        input_devices,
        input_group.device_id.as_deref(),
    ));
    input_window.set_selected_input_mode_index(input_mode_to_index(input_group.mode));
    input_window.set_status_message("".into());
}

pub(crate) fn apply_chain_output_window_state(
    output_window: &ChainOutputWindow,
    output_group: &OutputGroupDraft,
    output_devices: &[AudioDeviceDescriptor],
    channel_model: &Rc<VecModel<ChannelOptionItem>>,
) {
    replace_channel_options(
        channel_model,
        build_output_channel_items(output_group, output_devices),
    );
    output_window.set_selected_device_index(selected_device_index(
        output_devices,
        output_group.device_id.as_deref(),
    ));
    output_window.set_selected_output_mode_index(output_mode_to_index(output_group.mode));
    output_window.set_status_message("".into());
}
