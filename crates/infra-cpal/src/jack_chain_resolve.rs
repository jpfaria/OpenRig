//! Linux+JACK counterpart to `chain_resolve.rs`: build a synthetic
//! `ResolvedChainAudioConfig` from libjack's metadata for a chain.
//!
//! On the macOS / Windows path `chain_resolve.rs` resolves the chain
//! against a live `cpal::Host` (probing devices, picking supported
//! configs). On Linux+JACK we never touch ALSA — the JACK direct
//! backend takes over. `jack_resolve_chain_config` is the inverse:
//! given the chain and the controller's supervisor, it asks the
//! supervisor's cached meta for sample_rate, capture_port_count and
//! playback_port_count, and builds an empty (no inputs/outputs)
//! `ResolvedChainAudioConfig` whose only purpose is to carry the
//! sample_rate + stream_signature into `RuntimeGraph::upsert_chain`.
//!
//! Why a separate file: keeps `stream_builder.rs` under the 600-LOC
//! cap, and the function is conceptually closer to the resolution
//! family than to the cpal stream-building family that lives there.

#![cfg(all(target_os = "linux", feature = "jack"))]

use anyhow::{anyhow, Result};

use project::chain::Chain;

use crate::jack_supervisor;
use crate::resolved::{
    ChainStreamSignature, InputStreamSignature, OutputStreamSignature, ResolvedChainAudioConfig,
};
use crate::usb_proc::detect_all_usb_audio_cards;

/// Build a synthetic ResolvedChainAudioConfig using only the jack crate.
/// No CPAL or ALSA access. The resolved config is only used to provide
/// sample_rate and stream_signature to the runtime graph — the direct JACK
/// backend ignores inputs/outputs entirely.
///
/// Consumes cached meta from the supervisor — callers must guarantee that
/// `ensure_jack_servers` ran beforehand so every active card is in the
/// `Ready` state.
pub(crate) fn jack_resolve_chain_config(
    chain: &Chain,
    supervisor: &jack_supervisor::JackSupervisor<jack_supervisor::LiveJackBackend>,
) -> Result<ResolvedChainAudioConfig> {
    // Resolve the JACK server for this chain by inspecting its I/O device_ids.
    // Chain entries may have:
    //   - "jack:<server_name>"  → use that server directly
    //   - "hw:<N>"              → find the card at hw:N and use its server
    //   - anything else         → fall back to first supervised running server
    let cards = detect_all_usb_audio_cards();

    let supervisor_has_ready = |name: &str| {
        matches!(
            supervisor.state(&jack_supervisor::ServerName::from(name)),
            Some(jack_supervisor::JackServerState::Ready { .. })
        )
    };

    let resolve_server = |device_id: &str| -> Option<String> {
        if let Some(name) = device_id.strip_prefix("jack:") {
            return Some(name.to_string());
        }
        if let Some(hw_num) = device_id.strip_prefix("hw:") {
            if let Some(card) = cards.iter().find(|c| c.card_num == hw_num) {
                return Some(card.server_name.clone());
            }
        }
        cards
            .iter()
            .find(|c| supervisor_has_ready(&c.server_name))
            .map(|c| c.server_name.clone())
    };

    // Determine server from first input entry, or fallback to first
    // supervisor-ready card.
    let server_name = chain
        .input_blocks()
        .into_iter()
        .flat_map(|(_, ib)| ib.entries.iter())
        .find_map(|entry| resolve_server(&entry.device_id.0))
        .or_else(|| {
            cards
                .iter()
                .find(|c| supervisor_has_ready(&c.server_name))
                .map(|c| c.server_name.clone())
        })
        .ok_or_else(|| anyhow!("no running JACK server found for chain"))?;

    let meta = supervisor.meta(&jack_supervisor::ServerName::from(server_name.clone()))?;
    let device_id = format!("jack:{}", server_name);
    let sample_rate = meta.sample_rate as f32;
    let in_channels = meta.capture_port_count as u16;
    let out_channels = meta.playback_port_count as u16;

    let input_sigs: Vec<InputStreamSignature> = chain
        .input_blocks()
        .into_iter()
        .flat_map(|(_, ib)| ib.entries.iter())
        .map(|entry| InputStreamSignature {
            device_id: device_id.clone(),
            channels: entry.channels.clone(),
            stream_channels: in_channels,
            sample_rate: meta.sample_rate,
            buffer_size_frames: meta.buffer_size,
        })
        .collect();

    let output_sigs: Vec<OutputStreamSignature> = chain
        .output_blocks()
        .into_iter()
        .flat_map(|(_, ob)| ob.entries.iter())
        .map(|entry| OutputStreamSignature {
            device_id: device_id.clone(),
            channels: entry.channels.clone(),
            stream_channels: out_channels,
            sample_rate: meta.sample_rate,
            buffer_size_frames: meta.buffer_size,
        })
        .collect();

    Ok(ResolvedChainAudioConfig {
        inputs: Vec::new(),
        outputs: Vec::new(),
        sample_rate,
        stream_signature: ChainStreamSignature {
            inputs: input_sigs,
            outputs: output_sigs,
        },
    })
}
