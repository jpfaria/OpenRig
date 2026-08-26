//! Responsibility: projects the binding registry into the models the section binds to.

use crate::{ChannelOptionItem, IoBindingModel, IoEndpointModel};
use domain::io_binding::{IoBinding, IoEndpoint};
use domain::AudioDeviceDescriptor;
use slint::{Model, ModelRc, SharedString, VecModel};
use std::rc::Rc;

// ── Slint model projection ────────────────────────────────────────────────────

/// Shared Slint models the section renders, set on both window surfaces.
pub(crate) struct BindingModels {
    pub(crate) bindings: Rc<VecModel<IoBindingModel>>,
    /// Chain-level endpoint picker still consumes flat names on the main window.
    pub(crate) names: Rc<VecModel<SharedString>>,
    /// Channel checkboxes for the active add-row (rebuilt on device change).
    pub(crate) channels: Rc<VecModel<ChannelOptionItem>>,
}

pub(crate) fn endpoint_model(ep: &IoEndpoint) -> IoEndpointModel {
    use crate::ui_state::channels_label;
    IoEndpointModel {
        name: ep.name.as_str().into(),
        device_label: ep.device_id.0.as_str().into(),
        mode: super::io_bindings::io_bindings_endpoint::mode_label(ep.mode).into(),
        channels_label: channels_label(&ep.channels).into(),
    }
}

pub(crate) fn binding_model(b: &IoBinding) -> IoBindingModel {
    let inputs: Vec<IoEndpointModel> = b.inputs.iter().map(endpoint_model).collect();
    let outputs: Vec<IoEndpointModel> = b.outputs.iter().map(endpoint_model).collect();
    IoBindingModel {
        id: b.id.as_str().into(),
        name: b.name.as_str().into(),
        inputs: ModelRc::from(Rc::new(VecModel::from(inputs))),
        outputs: ModelRc::from(Rc::new(VecModel::from(outputs))),
    }
}

pub(crate) fn project_bindings(bindings: &[IoBinding]) -> Vec<IoBindingModel> {
    bindings.iter().map(binding_model).collect()
}

pub(crate) fn binding_names(bindings: &[IoBinding]) -> Vec<SharedString> {
    bindings
        .iter()
        .map(|b| SharedString::from(b.name.as_str()))
        .collect()
}

/// Re-project the binding list into the shared Slint models after any mutation.
pub(crate) fn reproject(models: &BindingModels, bindings: &[IoBinding]) {
    models.bindings.set_vec(project_bindings(bindings));
    models.names.set_vec(binding_names(bindings));
}

/// Build the (id, name) device-list models for one side from the live
/// descriptors. Empty when devices haven't been enumerated yet.
pub(crate) fn device_list_models(
    devices: &[AudioDeviceDescriptor],
) -> (Rc<VecModel<SharedString>>, Rc<VecModel<SharedString>>) {
    let ids = devices
        .iter()
        .map(|d| SharedString::from(d.id.as_str()))
        .collect::<Vec<_>>();
    let names = devices
        .iter()
        .map(|d| SharedString::from(d.name.as_str()))
        .collect::<Vec<_>>();
    (Rc::new(VecModel::from(ids)), Rc::new(VecModel::from(names)))
}

/// Currently-selected 0-based channel indices in the shared channel model.
pub(crate) fn selected_channels(channels: &Rc<VecModel<ChannelOptionItem>>) -> Vec<usize> {
    channels
        .iter()
        .filter(|c| c.selected)
        .map(|c| c.index as usize)
        .collect()
}
