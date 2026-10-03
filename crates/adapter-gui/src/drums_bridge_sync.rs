//! Responsibility: draws the drum machine's state onto one bridge.

use feature_dsp::drums::DrumPosition;
use slint::{ModelRc, SharedString, VecModel};

use crate::drums_picker_filter::filter_picks;
use crate::drums_view::{DrumPick, DrumsView};
use crate::{DrumPickRow, DrumsBridge};

fn rows(picks: &[DrumPick]) -> ModelRc<DrumPickRow> {
    let rows: Vec<DrumPickRow> = picks
        .iter()
        .map(|p| DrumPickRow {
            key: SharedString::from(p.key.as_str()),
            label: SharedString::from(p.label.as_str()),
            header: p.header,
        })
        .collect();
    ModelRc::new(VecModel::from(rows))
}

/// Publish the panel's choices (narrowed by the bridge's search), transport,
/// tempo and level.
pub(crate) fn set_drums_view(bridge: &DrumsBridge, view: &DrumsView) {
    bridge.set_enabled(view.enabled);
    bridge.set_playing(view.playing);
    bridge.set_bpm(view.bpm);
    bridge.set_bpm_min(view.bpm_min);
    bridge.set_bpm_max(view.bpm_max);
    bridge.set_volume(view.volume);
    bridge.set_beats_per_bar(view.beats_per_bar);
    let query = bridge.get_query();
    bridge.set_kit_rows(rows(&filter_picks(&view.kits, &query)));
    bridge.set_groove_rows(rows(&filter_picks(&view.grooves, &query)));
    bridge.set_output_rows(rows(&filter_picks(&view.outputs, &query)));
    bridge.set_kit_key(view.kit_key.as_str().into());
    bridge.set_kit_label(view.kit_label.as_str().into());
    bridge.set_groove_key(view.groove_key.as_str().into());
    bridge.set_groove_label(view.groove_label.as_str().into());
    bridge.set_output_key(view.output_key.as_str().into());
    bridge.set_output_label(view.output_label.as_str().into());
}

/// Light the beat lamps. No runtime reads as stopped, never a guessed beat.
pub(crate) fn set_drums_position(bridge: &DrumsBridge, position: Option<DrumPosition>) {
    let p = position.unwrap_or_default();
    bridge.set_current_bar(p.bar as i32);
    bridge.set_current_beat(p.beat as i32);
    bridge.set_in_fill(p.playing && p.in_fill);
}

#[cfg(test)]
#[path = "drums_bridge_sync_tests.rs"]
mod tests;
