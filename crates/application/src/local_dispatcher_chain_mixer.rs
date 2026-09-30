//! Responsibility: handles the commands of a chain's own mixer faders.
//! #1007: the compact chain view gives a chain one fader per endpoint it
//! plays through, on top of that endpoint's global strip, plus a DI fader.
//!
//! The faders are project data (`Chain.mix`, ADR 0003). A strip is addressed
//! by its global wire id; the handler maps it onto the chain's own port
//! (binding + endpoint name), which is what the project stores. Nothing here
//! touches the global strip or another chain.

use anyhow::{bail, Result};

use domain::mixer_gain::clamp_gain_db;
use domain::mixer_strip::{MixerDirection, MixerStripId};
use engine::chain_mix_gains::apply_chain_mix;
use project::binding_discovery::{resolve_chain_ports, PortDirection};
use project::chain::ChainEndpointMix;

use crate::command::{Command, MixerCommand};
use crate::event::Event;
use crate::local_dispatcher::LocalDispatcher;

type FaderChange = Box<dyn FnOnce(&mut ChainEndpointMix)>;

impl LocalDispatcher {
    pub(crate) fn handle_chain_mixer(&self, cmd: Command) -> Result<Vec<Event>> {
        let Command::Mixer(cmd) = cmd else {
            unreachable!("handle_chain_mixer received a non-mixer command: {cmd:?}");
        };
        let registry = self.current_io_bindings();
        let (chain, strip, change): (_, _, FaderChange) = match cmd {
            MixerCommand::SetChainDiFader { chain, gain_db } => {
                let gain_db = clamp_gain_db(gain_db);
                self.with_chain(&chain, |c| {
                    c.mix.di_gain_db = gain_db;
                    apply_chain_mix(c, &registry);
                    Ok(())
                })?;
                return Ok(vec![
                    Event::ChainDiFaderChanged { chain, gain_db },
                    Event::ProjectMutated,
                ]);
            }
            MixerCommand::SetChainMixerFader {
                chain,
                strip,
                gain_db,
            } => {
                let gain_db = clamp_gain_db(gain_db);
                (chain, strip, Box::new(move |f| f.gain_db = gain_db))
            }
            MixerCommand::SetChainMixerMute {
                chain,
                strip,
                muted,
            } => (chain, strip, Box::new(move |f| f.muted = muted)),
            MixerCommand::ToggleChainMixerMute { chain, strip } => {
                (chain, strip, Box::new(|f| f.muted = !f.muted))
            }
            other => unreachable!("global mixer command routed to a chain: {other:?}"),
        };
        let Some(id) = MixerStripId::parse(&strip) else {
            bail!("unknown mixer strip {strip:?}: expected in:<channels>@<device> or out:<channels>@<device>");
        };
        let (gain_db, muted) = self.with_chain(&chain, |c| {
            let Some(port) = resolve_chain_ports(c, &registry).into_iter().find(|port| {
                direction_of(port.direction) == id.direction
                    && port.endpoint.device_id.0 == id.device_id
                    && port.endpoint.channels == id.channels
            }) else {
                bail!("chain {:?} does not play through strip {strip:?}", c.id);
            };
            let fader = c
                .mix
                .endpoint_mut(id.direction, &port.binding_id, &port.endpoint.name);
            change(fader);
            let state = (fader.gain_db, fader.muted);
            c.mix.prune();
            apply_chain_mix(c, &registry);
            Ok(state)
        })?;
        Ok(vec![
            Event::ChainMixerStripChanged {
                chain,
                strip,
                gain_db,
                muted,
            },
            Event::ProjectMutated,
        ])
    }
}

fn direction_of(direction: PortDirection) -> MixerDirection {
    match direction {
        PortDirection::Input => MixerDirection::Input,
        PortDirection::Output => MixerDirection::Output,
    }
}
