//! #716: pure bridge between the chain editor's binding checklist and the
//! domain. No `AppWindow` — these are plain transforms (LAW 1/2).

use crate::ChainBindingChoice;
use domain::io_binding::IoBinding;

/// Build the checklist model: one row per registry binding, marked `selected`
/// when the chain already references it (by id).
pub fn binding_choices(registry: &[IoBinding], selected: &[String]) -> Vec<ChainBindingChoice> {
    registry
        .iter()
        .map(|b| ChainBindingChoice {
            id: b.id.as_str().into(),
            name: b.name.as_str().into(),
            selected: selected.iter().any(|s| s == &b.id),
        })
        .collect()
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
