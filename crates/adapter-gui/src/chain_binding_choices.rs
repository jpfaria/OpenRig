//! Responsibility: bridges the chain editor's binding checklist to the registry.
//! #716: pure bridge between the chain editor's binding checklist and the
//! domain. No `AppWindow` — these are plain transforms (LAW 1/2).

use crate::ChainBindingChoice;
use domain::io_binding::{IoBinding, IoEndpoint};

/// Build the checklist model: one row per registry binding, marked `selected`
/// when the chain already references it (by id).
pub fn binding_choices(registry: &[IoBinding], selected: &[String]) -> Vec<ChainBindingChoice> {
    registry
        .iter()
        .map(|b| ChainBindingChoice {
            id: b.id.as_str().into(),
            name: b.name.as_str().into(),
            selected: selected.iter().any(|s| s == &b.id),
            inputs_label: channels_label(&b.inputs).into(),
            outputs_label: channels_label(&b.outputs).into(),
        })
        .collect()
}

/// #398: the 1-based channels of every endpoint, joined by commas — the text
/// of a row's IN / OUT pill ("1", "1,2"); empty when there are none.
fn channels_label(endpoints: &[IoEndpoint]) -> String {
    endpoints
        .iter()
        .flat_map(|e| e.channels.iter())
        .map(|c| (c + 1).to_string())
        .collect::<Vec<_>>()
        .join(",")
}

/// Read the checked binding ids back out, preserving the checklist (registry)
/// order — this is what a saved chain's `io_binding_ids` becomes.
pub fn selected_binding_ids(choices: &[ChainBindingChoice]) -> Vec<String> {
    choices
        .iter()
        .filter(|c| c.selected)
        .map(|c| c.id.to_string())
        .collect()
}
