//! Responsibility: shapes the dispatcher's mixer strips into panel rows.
//! #1007: split by side, fader position and label pre-computed, so the panel
//! draws without any audio arithmetic of its own.

use application::mixer_view::MixerStripView;
use domain::mixer_solo::solo_silenced;
use domain::mixer_strip::MixerDirection;
use domain::mixer_strip_label::endpoint_channels_label;

use crate::mixer_fader_law::{gain_label, position_from_db};
use crate::MixerStripRow;

/// `(inputs, outputs)`, each in the order the dispatcher listed them.
pub(crate) fn mixer_rows(strips: &[MixerStripView]) -> (Vec<MixerStripRow>, Vec<MixerStripRow>) {
    let (inputs, outputs): (Vec<_>, Vec<_>) = strips
        .iter()
        .partition(|strip| strip.direction == MixerDirection::Input);
    (side_rows(&inputs), side_rows(&outputs))
}

/// One side's rows; a solo only dims the strips of its own side.
fn side_rows(strips: &[&MixerStripView]) -> Vec<MixerStripRow> {
    let side_has_solo = strips.iter().any(|strip| strip.soloed);
    strips
        .iter()
        .map(|strip| row(strip, side_has_solo))
        .collect()
}

fn row(strip: &MixerStripView, side_has_solo: bool) -> MixerStripRow {
    MixerStripRow {
        id: strip.id.as_str().into(),
        name: strip.name.as_str().into(),
        detail: endpoint_channels_label(strip.direction, &strip.channels).into(),
        is_input: strip.direction == MixerDirection::Input,
        position: position_from_db(strip.gain_db),
        gain_label: gain_label(strip.gain_db).into(),
        muted: strip.muted,
        soloed: strip.soloed,
        solo_silenced: solo_silenced(strip.soloed, side_has_solo),
    }
}

#[cfg(test)]
#[path = "mixer_rows_tests.rs"]
mod tests;
