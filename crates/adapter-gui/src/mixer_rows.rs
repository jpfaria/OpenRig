//! Responsibility: shapes the dispatcher's mixer strips into panel rows.
//! #1007: split by side, fader position and label pre-computed, so the panel
//! draws without any audio arithmetic of its own.

use application::mixer_view::MixerStripView;
use domain::mixer_strip::MixerDirection;

use crate::mixer_fader_law::{gain_label, position_from_db};
use crate::MixerStripRow;

/// `(inputs, outputs)`, each in the order the dispatcher listed them.
pub(crate) fn mixer_rows(strips: &[MixerStripView]) -> (Vec<MixerStripRow>, Vec<MixerStripRow>) {
    let (inputs, outputs): (Vec<_>, Vec<_>) = strips
        .iter()
        .partition(|strip| strip.direction == MixerDirection::Input);
    (
        inputs.into_iter().map(row).collect(),
        outputs.into_iter().map(row).collect(),
    )
}

fn row(strip: &MixerStripView) -> MixerStripRow {
    MixerStripRow {
        id: strip.id.as_str().into(),
        name: strip.name.as_str().into(),
        detail: channels_label(&strip.channels).into(),
        is_input: strip.direction == MixerDirection::Input,
        position: position_from_db(strip.gain_db),
        gain_label: gain_label(strip.gain_db).into(),
        muted: strip.muted,
    }
}

/// One-based, like the interface's own labels: "CH 15", "CH 1-2", "CH 1, 3".
fn channels_label(channels: &[usize]) -> String {
    let human: Vec<usize> = channels.iter().map(|c| c + 1).collect();
    let contiguous = human.windows(2).all(|w| w[1] == w[0] + 1);
    match human.as_slice() {
        [] => String::new(),
        [only] => format!("CH {only}"),
        [first, .., last] if contiguous => format!("CH {first}-{last}"),
        _ => {
            let list: Vec<String> = human.iter().map(ToString::to_string).collect();
            format!("CH {}", list.join(", "))
        }
    }
}

#[cfg(test)]
#[path = "mixer_rows_tests.rs"]
mod tests;
