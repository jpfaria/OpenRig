//! Responsibility: builds the option list of the DI panel's output select.
//! The DI plays to any output of the project — the same list the backing-track
//! player, the metronome and the looper offer (`project::project_outputs`) —
//! so a picked row maps to the endpoint it names, and the persisted
//! `Chain.di_output` is that endpoint's reference.

use std::rc::Rc;

use domain::io_binding::IoBinding;
use domain::AudioDeviceDescriptor;
use project::chain::{Chain, DiOutputRef};
use project::chain_output_choice::chain_output_index;
use project::project::Project;
use project::project_outputs::{output_endpoints, ProjectOutput};
use slint::{Model, ModelRc, SharedString, VecModel};

use crate::ProjectChainItem;

/// The reference the DI persists for the output on row `index`.
pub fn di_output_ref(outputs: &[ProjectOutput], index: usize) -> Option<DiOutputRef> {
    outputs.get(index).map(|o| DiOutputRef {
        binding_id: o.binding_id.clone(),
        endpoint: o.endpoint.clone(),
    })
}

/// Row-model convenience: the labels plus the selected index in one call
/// (what `ProjectChainItem.di_loop_outputs` / `.di_output_selected_index`
/// carry).
pub fn output_labels_and_index(
    chain: &Chain,
    registry: &[IoBinding],
    devices: &[AudioDeviceDescriptor],
) -> (Vec<String>, i32) {
    let outputs = output_endpoints(registry, devices);
    let index = di_output_selected_index(chain, registry, &outputs);
    (outputs.into_iter().map(|o| o.label).collect(), index)
}

/// Row of the chain's persisted `di_output`; `None` or a stale reference
/// select the chain's main output (what plays), `-1` when there is none.
pub fn di_output_selected_index(
    chain: &Chain,
    registry: &[IoBinding],
    outputs: &[ProjectOutput],
) -> i32 {
    let saved = chain
        .di_output
        .as_ref()
        .map(|r| (r.binding_id.as_str(), r.endpoint.as_str()));
    chain_output_index(chain, registry, outputs, saved)
}

/// #808: refresh the DI output select for EVERY chain row from the (real) I/O
/// bindings — regardless of whether the chain is active/enabled. The DI panel's
/// options are built inside `replace_project_chains`, but every caller passes an
/// empty binding registry, so a freshly opened project shows an EMPTY select;
/// the meter timer refreshes it with the real bindings but ONLY for chains that
/// produce a live audio reading, so a chain that was never enabled stayed empty
/// until the first enable. The options come from the project's bindings
/// (offline, no active stream needed — invariant #4), so populate them for all
/// chains here. Only the touched fields are written, and only when they change.
pub(crate) fn apply_di_outputs_to_rows(
    model: &VecModel<ProjectChainItem>,
    project: &Project,
    io_bindings: &[IoBinding],
    devices: &[AudioDeviceDescriptor],
) {
    for (idx, chain) in project.chains.iter().enumerate() {
        let Some(mut row) = model.row_data(idx) else {
            continue;
        };
        let (labels, selected) = output_labels_and_index(chain, io_bindings, devices);
        let current: Vec<String> = row.di_loop_outputs.iter().map(|s| s.to_string()).collect();
        let labels_changed = current != labels;
        let index_changed = row.di_output_selected_index != selected;
        if !labels_changed && !index_changed {
            continue;
        }
        if labels_changed {
            row.di_loop_outputs = ModelRc::from(Rc::new(VecModel::from(
                labels
                    .into_iter()
                    .map(SharedString::from)
                    .collect::<Vec<_>>(),
            )));
        }
        if index_changed {
            row.di_output_selected_index = selected;
        }
        model.set_row_data(idx, row);
    }
}

#[cfg(test)]
#[path = "di_output_options_tests.rs"]
mod tests;
