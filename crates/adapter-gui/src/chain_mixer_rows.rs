//! Responsibility: shapes one chain's own faders into strip rows.
//! #1007: the compact view pairs every global strip with the chain's own
//! fader on it, and adds the DI, LOOPER and MASTER single faders. Positions
//! and labels are computed here, so the strips hold no audio arithmetic.

use application::chain_fader_view::ChainFaderView;
use project::chain::Chain;
use project::looper::LooperConfig;

use crate::chain_level_law::{looper_mix_db, volume_db};
use crate::mixer_fader_law::{gain_label, position_from_db};
use crate::MixerStripRow;

/// Wire id of the chain's DI fader on `ChainMixerBridge`.
pub(crate) const DI_FADER_ID: &str = "di";
/// Wire id of the chain's MASTER (chain volume) fader.
pub(crate) const MASTER_FADER_ID: &str = "master";
/// Prefix of a looper fader id; the looper's uid follows.
pub(crate) const LOOPER_FADER_PREFIX: &str = "looper:";

/// The chain's own row for each `global` strip, in the same order. It keeps
/// the strip's label but none of its global state (level, mute, solo).
pub(crate) fn chain_side_rows(
    global: &[MixerStripRow],
    faders: &[ChainFaderView],
) -> Vec<MixerStripRow> {
    global
        .iter()
        .map(|strip| {
            let fader = faders.iter().find(|f| f.strip == strip.id.as_str());
            let gain_db = fader.map_or(0.0, |f| f.gain_db);
            MixerStripRow {
                position: position_from_db(gain_db),
                gain_label: gain_label(gain_db).into(),
                muted: fader.is_some_and(|f| f.muted),
                soloed: false,
                solo_silenced: false,
                ..strip.clone()
            }
        })
        .collect()
}

fn single_row(id: String, name: String, detail: String, gain_db: f32) -> MixerStripRow {
    MixerStripRow {
        id: id.into(),
        name: name.into(),
        detail: detail.into(),
        is_input: false,
        position: position_from_db(gain_db),
        gain_label: gain_label(gain_db).into(),
        muted: false,
        soloed: false,
        solo_silenced: false,
    }
}

pub(crate) fn di_row(gain_db: f32) -> MixerStripRow {
    single_row(
        DI_FADER_ID.into(),
        rust_i18n::t!("mixer-strip-di").into(),
        String::new(),
        gain_db,
    )
}

pub(crate) fn master_row(volume_percent: f32) -> MixerStripRow {
    single_row(
        MASTER_FADER_ID.into(),
        rust_i18n::t!("mixer-strip-master").into(),
        format!("{}%", volume_percent.round() as i32),
        volume_db(volume_percent),
    )
}

pub(crate) fn looper_rows(loopers: &[LooperConfig]) -> Vec<MixerStripRow> {
    loopers
        .iter()
        .enumerate()
        .map(|(i, looper)| {
            single_row(
                format!("{LOOPER_FADER_PREFIX}{}", looper.uid),
                rust_i18n::t!("mixer-strip-looper", n = i + 1).into(),
                String::new(),
                looper_mix_db(looper.mix),
            )
        })
        .collect()
}

/// Every tab of one chain's compact mixer.
pub(crate) struct ChainMixerRows {
    pub inputs: Vec<MixerStripRow>,
    pub outputs: Vec<MixerStripRow>,
    pub di: Vec<MixerStripRow>,
    pub loopers: Vec<MixerStripRow>,
    pub master: Vec<MixerStripRow>,
}

/// The chain rows beside the `global_inputs` / `global_outputs` strips it
/// plays through, and its DI, LOOPER and MASTER faders.
pub(crate) fn chain_mixer_rows(
    global_inputs: &[MixerStripRow],
    global_outputs: &[MixerStripRow],
    faders: &[ChainFaderView],
    chain: &Chain,
) -> ChainMixerRows {
    ChainMixerRows {
        inputs: chain_side_rows(global_inputs, faders),
        outputs: chain_side_rows(global_outputs, faders),
        di: vec![di_row(chain.mix.di_gain_db)],
        loopers: looper_rows(&chain.loopers),
        master: vec![master_row(chain.volume)],
    }
}

#[cfg(test)]
#[path = "chain_mixer_rows_tests.rs"]
mod tests;
