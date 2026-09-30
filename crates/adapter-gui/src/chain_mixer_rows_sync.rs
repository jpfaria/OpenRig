//! Responsibility: draws one chain's own fader rows into a `ChainMixerBridge` in place.
//! #1007 — same rule as the global strips (`mixer_rows_sync`): rows that keep
//! their ids are updated row by row, so a chain fader held under the pointer
//! keeps its grab while the compact view redraws.

use crate::chain_mixer_rows::ChainMixerRows;
use crate::mixer_rows_sync::updated_in_place;
use crate::ChainMixerBridge;

pub(crate) fn set_chain_mixer_rows(bridge: &ChainMixerBridge, rows: ChainMixerRows) {
    if let Some(model) = updated_in_place(bridge.get_inputs(), rows.inputs) {
        bridge.set_inputs(model);
    }
    if let Some(model) = updated_in_place(bridge.get_outputs(), rows.outputs) {
        bridge.set_outputs(model);
    }
    if let Some(model) = updated_in_place(bridge.get_di(), rows.di) {
        bridge.set_di(model);
    }
    if let Some(model) = updated_in_place(bridge.get_loopers(), rows.loopers) {
        bridge.set_loopers(model);
    }
    if let Some(model) = updated_in_place(bridge.get_master(), rows.master) {
        bridge.set_master(model);
    }
}
