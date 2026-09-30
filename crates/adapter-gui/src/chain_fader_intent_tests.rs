//! #1007: what a move of one of the chain's single faders asks the app to do.

use application::command::{Command, LooperCommand, LooperParam, MixerCommand};
use domain::ids::ChainId;

use super::*;
use crate::chain_mixer_rows::{DI_FADER_ID, MASTER_FADER_ID};

fn chain() -> ChainId {
    ChainId("c".into())
}

#[test]
fn the_di_fader_sets_the_chain_di_gain() {
    let intent = single_fader_intent(&chain(), DI_FADER_ID, -12.0);
    assert!(matches!(
        intent,
        Some(ChainFaderIntent::Dispatch(Command::Mixer(MixerCommand::SetChainDiFader { chain, gain_db })))
            if chain.0 == "c" && gain_db == -12.0
    ));
}

#[test]
fn the_master_fader_sets_the_chain_volume_percent() {
    let intent = single_fader_intent(&chain(), MASTER_FADER_ID, -6.0);
    assert!(matches!(intent, Some(ChainFaderIntent::MasterVolume(50))));
}

#[test]
fn a_looper_fader_sets_that_looper_mix() {
    let intent = single_fader_intent(&chain(), "looper:7", 0.0);
    assert!(matches!(
        intent,
        Some(ChainFaderIntent::Dispatch(Command::Looper(LooperCommand::SetChainLooperParam {
            chain,
            looper: 7,
            param: LooperParam::Mix(mix),
        }))) if chain.0 == "c" && mix == 1.0
    ));
}

#[test]
fn an_unknown_fader_asks_for_nothing() {
    assert!(single_fader_intent(&chain(), "looper:x", 0.0).is_none());
    assert!(single_fader_intent(&chain(), "reverb", 0.0).is_none());
}
