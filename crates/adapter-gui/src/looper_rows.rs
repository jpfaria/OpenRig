//! Responsibility: writes the looper state into the chain rows on screen.

use crate::looper_items::{any_looper_active, looper_items, looper_items_from_config};

use engine::LooperStatus;
use project::chain::Chain;

/// Populate every chain row's looper Record-from / Play-to endpoint options
/// from the live bindings — offline, no device needed, so a chain that is not
/// started still shows its options (mirrors the DI output picker's
/// `apply_di_outputs_to_rows`). Writes a row back only when its options change.
pub fn apply_looper_endpoints_to_rows(
    project_chains: &slint::VecModel<crate::ProjectChainItem>,
    project: &project::project::Project,
    registry: &[domain::io_binding::IoBinding],
) {
    use slint::Model;
    for (idx, chain) in project.chains.iter().enumerate() {
        let Some(mut row) = project_chains.row_data(idx) else {
            continue;
        };
        let (inputs, outputs) = project::binding_discovery::chain_endpoint_labels(chain, registry);
        let cur_in: Vec<String> = row
            .looper_input_options
            .iter()
            .map(|s| s.to_string())
            .collect();
        let cur_out: Vec<String> = row
            .looper_output_options
            .iter()
            .map(|s| s.to_string())
            .collect();
        if cur_in == inputs && cur_out == outputs {
            continue;
        }
        row.looper_input_options = slint::ModelRc::from(std::rc::Rc::new(slint::VecModel::from(
            inputs
                .into_iter()
                .map(slint::SharedString::from)
                .collect::<Vec<_>>(),
        )));
        row.looper_output_options = slint::ModelRc::from(std::rc::Rc::new(slint::VecModel::from(
            outputs
                .into_iter()
                .map(slint::SharedString::from)
                .collect::<Vec<_>>(),
        )));
        project_chains.set_row_data(idx, row);
    }
}

/// #323 phase 2: populate every chain row's looper preset options — the chain's
/// bank preset names, in slot order — so the drawer's preset picker lists them
/// even on a chain that is not started (mirrors `apply_looper_endpoints_to_rows`).
/// The "follow the chain" entry is option 0 and is added by the Slint picker
/// (@tr, so it stays translatable); these are the bank names for options 1..N,
/// matching each loop's `preset_index` (0 = follow, k = bank slot k). A non-rig
/// chain gets an empty list. Writes a row back only when its options change.
pub fn apply_looper_presets_to_rows(
    project_chains: &slint::VecModel<crate::ProjectChainItem>,
    project: &project::project::Project,
    rig: Option<&std::cell::RefCell<project::rig::RigProject>>,
) {
    use slint::Model;
    for (idx, chain) in project.chains.iter().enumerate() {
        let Some(mut row) = project_chains.row_data(idx) else {
            continue;
        };
        let options: Vec<String> = match rig {
            Some(rig) => crate::chain_preset_wiring::chain_preset_bank(&chain.id, &rig.borrow())
                .into_iter()
                .map(|(_, label)| label)
                .collect(),
            None => Vec::new(),
        };
        let cur: Vec<String> = row
            .looper_preset_options
            .iter()
            .map(|s| s.to_string())
            .collect();
        if cur == options {
            continue;
        }
        row.looper_preset_options = slint::ModelRc::from(std::rc::Rc::new(slint::VecModel::from(
            options
                .into_iter()
                .map(slint::SharedString::from)
                .collect::<Vec<_>>(),
        )));
        project_chains.set_row_data(idx, row);
    }
}

/// #323 phase 2: the bank preset ids (option order) a chain's loopers map their
/// `preset_index` against. Empty for a non-rig chain.
pub(crate) fn chain_preset_ids(
    chain: &Chain,
    rig: Option<&std::cell::RefCell<project::rig::RigProject>>,
) -> Vec<String> {
    match rig {
        Some(rig) => crate::chain_preset_wiring::chain_preset_bank(&chain.id, &rig.borrow())
            .into_iter()
            .map(|(id, _)| id)
            .collect(),
        None => Vec::new(),
    }
}

/// Rebuild the looper rows of one chain-card row in place.
///
/// `sample_rate` is `Some` only when the chain has a live runtime — then the
/// live `statuses` fill state/position/length; `None` means "no stream yet",
/// and the rows come from the persisted config alone (no fictional rate). Used
/// both by the meter timer (live) and by the panel callbacks so an add /
/// remove reflects immediately even on a chain with no running stream — the
/// path that let the config accumulate to the 8-looper cap with an empty
/// panel.
pub fn write_chain_looper_row(
    project_chains: &slint::VecModel<crate::ProjectChainItem>,
    index: usize,
    chain: &Chain,
    statuses: &[LooperStatus],
    sample_rate: Option<u32>,
    registry: &[domain::io_binding::IoBinding],
    preset_ids: &[String],
) {
    use slint::Model;
    let Some(mut row) = project_chains.row_data(index) else {
        return;
    };
    // `Some(rate)` is passed only when the chain has a LIVE runtime (see the
    // callers), so it doubles as the "REC is armable" signal.
    let rows = match sample_rate {
        Some(rate) => looper_items(chain, statuses, rate, registry, true, preset_ids),
        None => looper_items_from_config(chain, registry, preset_ids),
    };
    row.looper_active = any_looper_active(&rows);
    row.loopers = slint::ModelRc::from(std::rc::Rc::new(slint::VecModel::from(rows)));
    project_chains.set_row_data(index, row);
}
