//! Responsibility: decides what a move of one of a chain's single faders asks the app to do.
//! #1007: the DI and LOOPER faders are plain commands; MASTER is the chain
//! volume, which the compact view already routes through its header path
//! (live sync, chain list, unsaved marker), so it is handed back as a value.

use application::command::{Command, LooperCommand, LooperParam, MixerCommand};
use domain::ids::ChainId;

use crate::chain_level_law::{looper_mix, volume_percent};
use crate::chain_mixer_rows::{DI_FADER_ID, LOOPER_FADER_PREFIX, MASTER_FADER_ID};

pub(crate) enum ChainFaderIntent {
    Dispatch(Command),
    /// New chain volume, percent (0..200).
    MasterVolume(i32),
}

pub(crate) fn single_fader_intent(
    chain: &ChainId,
    fader: &str,
    gain_db: f32,
) -> Option<ChainFaderIntent> {
    if fader == DI_FADER_ID {
        return Some(ChainFaderIntent::Dispatch(Command::Mixer(
            MixerCommand::SetChainDiFader {
                chain: chain.clone(),
                gain_db,
            },
        )));
    }
    if fader == MASTER_FADER_ID {
        return Some(ChainFaderIntent::MasterVolume(volume_percent(gain_db)));
    }
    let uid = fader.strip_prefix(LOOPER_FADER_PREFIX)?.parse().ok()?;
    Some(ChainFaderIntent::Dispatch(Command::Looper(
        LooperCommand::SetChainLooperParam {
            chain: chain.clone(),
            looper: uid,
            param: LooperParam::Mix(looper_mix(gain_db)),
        },
    )))
}

#[cfg(test)]
#[path = "chain_fader_intent_tests.rs"]
mod tests;
