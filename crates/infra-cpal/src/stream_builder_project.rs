//! Responsibility: builds every enabled chain's cpal streams for a whole project.
//!
//! The bulk/diagnostic entry point: callers that want a flat `Vec<Stream>`
//! (test harnesses, console tools) instead of per-chain `ActiveChainRuntime`s.
//! No-op on Linux+JACK, where every byte goes through the JACK direct backend.

#[cfg(not(all(target_os = "linux", feature = "jack")))]
use anyhow::anyhow;
use anyhow::Result;

use cpal::Stream;

use domain::io_binding::IoBinding;

#[cfg(not(all(target_os = "linux", feature = "jack")))]
use crate::build_chain_slots;
#[cfg(not(all(target_os = "linux", feature = "jack")))]
use crate::stream_builder::build_chain_streams;

pub fn build_streams_for_project(
    project: &project::project::Project,
    runtime_graph: &engine::runtime::RuntimeGraph,
    registry: &[IoBinding],
) -> Result<Vec<Stream>> {
    log::info!("building audio streams for project");

    // On Linux with JACK, no CPAL streams are ever needed — streaming is handled
    // entirely by the jack crate in build_active_chain_runtime. Also, calling
    // validate_channels_against_devices() here would probe ALSA PCM and disturb
    // USB audio devices.
    #[cfg(all(target_os = "linux", feature = "jack"))]
    {
        let _ = project; // not needed on Linux/JACK
        let _ = runtime_graph; // not needed on Linux/JACK: all streaming handled by jack crate
        let _ = registry; // not needed on Linux/JACK: device endpoints come from libjack meta
        return Ok(Vec::new());
    }

    #[cfg(not(all(target_os = "linux", feature = "jack")))]
    {
        let host = crate::host::get_host();
        crate::validation::validate_channels_against_devices(project, host, registry)?;
        let mut resolved_chains =
            crate::chain_resolve::resolve_enabled_chain_audio_configs(host, project, registry)?;
        let mut streams = Vec::new();
        for chain in &project.chains {
            if !chain.enabled {
                continue;
            }
            // Issue #350 phase 3: a chain owns N per-input runtimes (one
            // per physical input device). Pass the full ordered
            // (group_id, runtime) list so each input cpal stream feeds
            // runtime (chain, group) and the output cpal stream mixes them
            // at the backend. Single-input chains have exactly one entry
            // here and take the byte-identical fast path.
            let runtimes = runtime_graph.runtimes_with_groups_for(&chain.id);
            if runtimes.is_empty() {
                return Err(anyhow!("chain '{}' has no runtime state", chain.id.0));
            }
            // This bulk/console path has no controller to hold the slots, so the
            // wrappers are throwaway (no live swap needed here); the streams
            // still read through them identically.
            let slots = build_chain_slots(&runtimes);
            let resolved = resolved_chains
                .remove(&chain.id)
                .ok_or_else(|| anyhow!("chain '{}' missing resolved audio config", chain.id.0))?;
            let (input_streams, output_streams) =
                build_chain_streams(&chain.id, resolved, slots, &[])?;
            streams.extend(input_streams);
            streams.extend(output_streams);
        }
        Ok(streams)
    }
}
