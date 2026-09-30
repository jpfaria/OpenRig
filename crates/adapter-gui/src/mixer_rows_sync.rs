//! Responsibility: draws the mixer rows into a `MixerBridge` in place.
//! #1007 — the open mixer redraws the strips from the dispatcher after every
//! gesture and on its poll. Replacing the model recreates every strip, so a
//! fader held under the pointer lost its grab mid-drag ("faders feel stuck").
//! Rows that keep their strip ids are updated row by row instead; only a
//! different set of strips gets a new model.

use slint::{Model, ModelRc, VecModel};

use crate::{MixerBridge, MixerStripRow};

/// Show `inputs` and `outputs` on `bridge`.
pub fn set_mixer_rows(
    bridge: &MixerBridge,
    inputs: Vec<MixerStripRow>,
    outputs: Vec<MixerStripRow>,
) {
    if let Some(model) = updated_in_place(bridge.get_inputs(), inputs) {
        bridge.set_inputs(model);
    }
    if let Some(model) = updated_in_place(bridge.get_outputs(), outputs) {
        bridge.set_outputs(model);
    }
}

/// Write `rows` into `current` row by row when both list the same strips;
/// otherwise the new model to install.
fn updated_in_place(
    current: ModelRc<MixerStripRow>,
    rows: Vec<MixerStripRow>,
) -> Option<ModelRc<MixerStripRow>> {
    let same_strips = current.row_count() == rows.len()
        && current.iter().zip(&rows).all(|(old, new)| old.id == new.id);
    if !same_strips {
        return Some(ModelRc::new(VecModel::from(rows)));
    }
    for (i, row) in rows.into_iter().enumerate() {
        if current.row_data(i).as_ref() != Some(&row) {
            current.set_row_data(i, row);
        }
    }
    None
}
