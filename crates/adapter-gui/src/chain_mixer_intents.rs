//! Responsibility: turns gestures on a `ChainMixerBridge` into chain fader intents.
//! #1007 — the chain's own fader on an endpoint is a chain mixer command; the
//! single faders (DI, LOOPER, MASTER) are decided by `chain_fader_intent`. A
//! double-click puts any of them back at unity (0 dB).

use std::rc::Rc;

use application::command::{Command, MixerCommand};
use domain::ids::ChainId;

use crate::chain_fader_intent::{single_fader_intent, ChainFaderIntent};
use crate::mixer_fader_law::db_from_position;
use crate::ChainMixerBridge;

/// The chain the compact view shows, read at gesture time.
pub(crate) type ChainOf = Rc<dyn Fn() -> Option<ChainId>>;
/// Carries an intent out (dispatch, or the chain volume path).
pub(crate) type Act = Rc<dyn Fn(ChainFaderIntent)>;

pub(crate) fn wire_chain_mixer_intents(bridge: &ChainMixerBridge, chain: ChainOf, act: Act) {
    let strip = {
        let (chain, act) = (chain.clone(), act.clone());
        Rc::new(move |make: &dyn Fn(ChainId) -> MixerCommand| {
            if let Some(id) = chain() {
                act(ChainFaderIntent::Dispatch(Command::Mixer(make(id))));
            }
        })
    };
    let s = strip.clone();
    bridge.on_chain_fader_moved(move |id, position| {
        s(&|chain| MixerCommand::SetChainMixerFader {
            chain,
            strip: id.to_string(),
            gain_db: db_from_position(position),
        });
    });
    let s = strip.clone();
    bridge.on_chain_fader_reset(move |id| {
        s(&|chain| MixerCommand::SetChainMixerFader {
            chain,
            strip: id.to_string(),
            gain_db: 0.0,
        });
    });
    bridge.on_chain_mute_toggled(move |id| {
        strip(&|chain| MixerCommand::ToggleChainMixerMute {
            chain,
            strip: id.to_string(),
        });
    });

    let single = Rc::new(move |fader: &str, gain_db: f32| {
        let Some(id) = chain() else {
            return;
        };
        if let Some(intent) = single_fader_intent(&id, fader, gain_db) {
            act(intent);
        }
    });
    let s = single.clone();
    bridge.on_single_fader_moved(move |id, position| s(&id, db_from_position(position)));
    bridge.on_single_fader_reset(move |id| single(&id, 0.0));
}

#[cfg(test)]
#[path = "chain_mixer_intents_tests.rs"]
mod tests;
