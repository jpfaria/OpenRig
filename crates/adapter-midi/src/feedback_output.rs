//! Responsibility: sends mixer feedback bytes to the controller's MIDI outputs.
//! #1007 — the return path of a bidirectional surface: motor faders and LEDs.
//! Runs on the GUI thread's feedback timer, never on the audio thread.

use anyhow::{anyhow, Context, Result};
use midir::{MidiOutput, MidiOutputConnection};

const CLIENT_NAME: &str = "OpenRig";
const PORT_NAME: &str = "openrig-midi-feedback";

/// Every MIDI output a controller is reached on.
pub struct FeedbackOutput {
    connections: Vec<MidiOutputConnection>,
}

impl FeedbackOutput {
    /// Open every output whose name contains `wanted` (the map's `input`
    /// name — a surface's in and out ports share it). `None`, or no match,
    /// is an error: the caller simply runs without feedback.
    pub fn open(wanted: Option<&str>) -> Result<Self> {
        let probe = MidiOutput::new(CLIENT_NAME).context("creating MIDI output client")?;
        let names: Vec<String> = probe
            .ports()
            .iter()
            .map(|p| probe.port_name(p).unwrap_or_default())
            .collect();
        let mut connections = Vec::new();
        for idx in feedback_port_indices(&names, wanted) {
            let client = MidiOutput::new(CLIENT_NAME).context("creating MIDI output client")?;
            let ports = client.ports();
            let Some(port) = ports.get(idx) else {
                continue;
            };
            match client.connect(port, PORT_NAME) {
                Ok(conn) => {
                    log::info!("adapter-midi: mixer feedback to '{}'", names[idx]);
                    connections.push(conn);
                }
                Err(e) => log::warn!("adapter-midi: feedback port '{}': {e}", names[idx]),
            }
        }
        if connections.is_empty() {
            return Err(anyhow!(
                "no MIDI output matched {wanted:?} (available: {names:?})"
            ));
        }
        Ok(Self { connections })
    }

    /// Send one message to every opened output. A failed send is logged and
    /// skipped — a missing LED must never stop the rig.
    pub fn send(&mut self, message: &[u8]) {
        for conn in &mut self.connections {
            if let Err(e) = conn.send(message) {
                log::warn!("adapter-midi: feedback send failed: {e}");
            }
        }
    }
}

/// Indices of the outputs to send feedback to: every name containing
/// `wanted` (case-insensitive). No name selects nothing, so feedback never
/// reaches gear the map did not name.
pub fn feedback_port_indices(available: &[String], wanted: Option<&str>) -> Vec<usize> {
    let Some(wanted) = wanted else {
        return Vec::new();
    };
    let wanted = wanted.to_lowercase();
    available
        .iter()
        .enumerate()
        .filter(|(_, name)| name.to_lowercase().contains(&wanted))
        .map(|(i, _)| i)
        .collect()
}

#[cfg(test)]
#[path = "feedback_output_tests.rs"]
mod tests;
