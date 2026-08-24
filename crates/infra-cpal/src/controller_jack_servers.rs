//! Responsibility: keeps every card's jackd in the configuration the project asks for.
//!
//! Issue #308: the supervisor owns the processes; this is the controller-side
//! translation from project device settings to `JackConfig` plus the ordered
//! teardown a restart demands.

use anyhow::{bail, Result};

use project::project::Project;

use crate::controller::ProjectRuntimeController;
use crate::jack_supervisor;
use crate::usb_proc::{detect_all_usb_audio_cards, UsbAudioCard};

impl ProjectRuntimeController {
    /// Translate a detected USB audio card + project-level device settings
    /// into a [`jack_supervisor::JackConfig`] suitable for `ensure_server`.
    /// Kept as a free helper on the controller so `sync_project` and
    /// `upsert_chain` share the same translation.
    #[cfg(all(target_os = "linux", feature = "jack"))]
    pub(crate) fn jack_config_for_card(
        card: &UsbAudioCard,
        project: &Project,
    ) -> jack_supervisor::JackConfig {
        let matched = project
            .device_settings
            .iter()
            .find(|s| s.device_id.0 == card.device_id);
        let sample_rate = matched.map(|s| s.sample_rate).unwrap_or(48_000);
        // Unconfigured-device fallback. 64 frames continuously xruns on a
        // generic (non-RT) desktop kernel with USB audio — the stream
        // smears into a dull "muffled / in a bag" sound (NOT clicks),
        // which is exactly what users reported (#479). 256 is the safe
        // USB minimum. Configured setups carry an explicit value so this
        // is never hit (Orange Pi unaffected). THIS is the path that
        // builds the live JackConfig — device_settings.rs has a twin
        // fallback (kept in sync; both 256).
        let buffer_size = matched.map(|s| s.buffer_size_frames).unwrap_or(256);
        let nperiods = matched.map(|s| s.nperiods).unwrap_or(3);
        let realtime = matched.map(|s| s.realtime).unwrap_or(true);
        let rt_priority = matched.map(|s| s.rt_priority).unwrap_or(70);
        jack_supervisor::JackConfig {
            sample_rate,
            buffer_size,
            nperiods,
            realtime,
            rt_priority,
            card_num: card.card_num.parse().unwrap_or(0),
            capture_channels: card.capture_channels,
            playback_channels: card.playback_channels,
        }
    }

    /// Ensure every connected card has its jackd in the desired config. When
    /// a restart will be triggered for any card that still has active chains,
    /// drop the chains first — dropping an `AsyncClient` after its jackd has
    /// been SIGTERMed leaves the libjack global state in the
    /// `ClientStatus(FAILURE | SERVER_ERROR)` limbo documented in issue #294.
    #[cfg(all(target_os = "linux", feature = "jack"))]
    fn ensure_jack_servers(&mut self, project: &Project) -> Result<()> {
        let cards = detect_all_usb_audio_cards();
        if cards.is_empty() {
            bail!("no USB audio interface found — connect a device before starting audio");
        }

        let configs: Vec<(jack_supervisor::ServerName, jack_supervisor::JackConfig)> = cards
            .iter()
            .map(|card| {
                (
                    jack_supervisor::ServerName::from(card.server_name.clone()),
                    Self::jack_config_for_card(card, project),
                )
            })
            .collect();

        // Fast path — buffer-only deltas go through jack_set_buffer_size
        // on a live client, no jackd restart, no libjack state corruption.
        // This is the behaviour the user already has on macOS/CoreAudio:
        // change the buffer and audio continues without interruption.
        let mut remaining: Vec<(&jack_supervisor::ServerName, &jack_supervisor::JackConfig)> =
            Vec::with_capacity(configs.len());
        for (name, cfg) in &configs {
            if self.supervisor.only_buffer_changed(name, cfg) {
                let server_device_id = format!("jack:{}", name);
                let live_client = self.active_chains.values().find(|ac| {
                    ac.stream_signature
                        .inputs
                        .first()
                        .map(|s| s.device_id.as_str() == server_device_id)
                        .unwrap_or(false)
                });
                match live_client {
                    Some(ac) => match ac.set_live_buffer_size(cfg.buffer_size) {
                        Ok(()) => {
                            self.supervisor.mark_buffer_resized(name, cfg.buffer_size);
                            log::info!(
                                "ensure_jack_servers: '{}' buffer_size → {} applied live (no restart)",
                                name,
                                cfg.buffer_size
                            );
                            continue;
                        }
                        Err(e) => {
                            log::warn!(
                                "ensure_jack_servers: live buffer resize failed on '{}' ({}), falling back to restart",
                                name,
                                e
                            );
                        }
                    },
                    None => {
                        log::debug!(
                            "ensure_jack_servers: no live client bound to '{}', skipping soft resize",
                            name
                        );
                    }
                }
            }
            remaining.push((name, cfg));
        }

        let any_would_restart = remaining
            .iter()
            .any(|(name, cfg)| self.supervisor.would_restart(name, cfg));
        if any_would_restart && !self.active_chains.is_empty() {
            log::info!(
                "ensure_jack_servers: JACK restart imminent, tearing down {} chain(s) first",
                self.active_chains.len()
            );
            self.stop();
            // Give libjack's client-side threads a moment to finish winding
            // down after `jack_deactivate` / `jack_client_close`. Without
            // this, killing jackd immediately after dropping AsyncClients
            // has been observed to leave libjack process-wide state confused
            // and the next `Client::new` fails with "Cannot open shm
            // segment" (issue #294 / #308). 500 ms is the shortest delay
            // that reliably clears the residual threads on the deployment
            // targets we test against.
            std::thread::sleep(std::time::Duration::from_millis(500));
        }

        for (name, config) in remaining {
            // The predictive teardown above already cleared any active chains
            // bound to a restarting server. The hook is a safety net.
            let mut hook = |_: &jack_supervisor::ServerName| {};
            self.supervisor.ensure_server(name, config, &mut hook)?;
        }
        Ok(())
    }
}
