//! Responsibility: assembles one chain's cpal streams into its ActiveChainRuntime.
//!
//! `build_active_chain_runtime` is the entry point the controller calls:
//! on Linux+JACK it routes to `build_jack_direct_chain`, otherwise it
//! stitches together the per-input streams (`stream_builder_input`) and
//! the per-output streams (`stream_builder_output`) for the chain.

#[cfg(not(all(target_os = "linux", feature = "jack")))]
use anyhow::anyhow;
use anyhow::Result;

#[cfg(not(all(target_os = "linux", feature = "jack")))]
use cpal::traits::{DeviceTrait, StreamTrait};
use cpal::Stream;

use domain::ids::ChainId;
use domain::io_binding::IoBinding;
use project::chain::Chain;

use crate::active_runtime::ActiveChainRuntime;
use crate::resolved::ResolvedChainAudioConfig;
#[cfg(not(all(target_os = "linux", feature = "jack")))]
use crate::stream_builder_input::build_input_stream_for_input;
#[cfg(not(all(target_os = "linux", feature = "jack")))]
pub(crate) use crate::stream_builder_output::build_output_stream_for_output;
use crate::LiveRuntimeSlot;

#[cfg(all(target_os = "linux", feature = "jack"))]
use crate::host::jack_server_is_running;
#[cfg(all(target_os = "linux", feature = "jack"))]
use crate::jack_direct::build_jack_direct_chain;

/// Stitch the per-input + per-output cpal streams for one chain.
///
/// Issue #350 phase 3: `runtimes` is the chain's ordered list of
/// per-input runtimes — `(group_id, ChainRuntimeState)` where `group_id`
/// is the cpal input index that runtime owns (see
/// `RuntimeGraph::runtimes_with_groups_for`). The engine's
/// `effective_inputs` assigns cpal indices by first-seen distinct device
/// over the chain's raw input entries; `resolved.inputs` is in that same
/// raw-entry order, so deduplicating it by device in iteration order
/// yields the Nth distinct device == group N. Each physical input device
/// therefore gets its OWN cpal stream bound to its OWN runtime
/// `(chain, group)` — never collapsed to the first. The shared output
/// device's stream is handed EVERY runtime and sums them at the backend
/// (the only mix point invariant #4 permits).
#[cfg(not(all(target_os = "linux", feature = "jack")))]
pub(crate) fn build_chain_streams(
    chain_id: &ChainId,
    resolved: ResolvedChainAudioConfig,
    slots: Vec<(usize, LiveRuntimeSlot)>,
    _di_cells: &[crate::di_playback::DiPlaybackCell], // #808: chain output is DI-free now
) -> Result<(Vec<Stream>, Vec<Stream>)> {
    // Flat list (group order) for the backend output mix. Issue #672: the
    // callbacks read each slot live so a worker-published rebuild takes
    // effect without a stream rebuild.
    let all_slots: Vec<LiveRuntimeSlot> = slots.iter().map(|(_, s)| s.handle()).collect();
    // Fallback used only if a chain somehow has no per-input runtime for a
    // given cpal index (degenerate config) — keeps behaviour defined
    // instead of panicking on the audio-setup path.
    let first_slot = all_slots.first().cloned();

    // Deduplicate input streams by device: one CPAL stream per unique
    // device. Iteration order over resolved.inputs matches the engine's
    // first-seen-device cpal-index assignment, so the Nth distinct device
    // is cpal index N. Issue #703: that one stream binds EVERY per-entry
    // runtime fed by this device (two entries on one interface are two
    // isolated runtimes sharing the stream) — `slots_for_input_stream`
    // resolves the set.
    let mut input_streams = Vec::new();
    let mut seen_devices: std::collections::HashSet<String> = std::collections::HashSet::new();
    let mut next_cpal_index: usize = 0;
    for (i, resolved_input) in resolved.inputs.into_iter().enumerate() {
        let device_key = resolved_input
            .device
            .id()
            .map(|id| id.to_string())
            .unwrap_or_default();
        if !seen_devices.insert(device_key.clone()) {
            log::info!(
                "input[{}] shares device '{}', reusing existing CPAL stream",
                i,
                device_key
            );
            continue;
        }
        let cpal_index = next_cpal_index;
        next_cpal_index += 1;
        let mut bound = crate::slot_processing::slots_for_input_stream(&slots, cpal_index);
        if bound.is_empty() {
            let fallback = first_slot.as_ref().map(|s| s.handle()).ok_or_else(|| {
                anyhow!(
                    "chain '{}' cpal input {} has no per-input runtime",
                    chain_id.0,
                    cpal_index
                )
            })?;
            bound.push(fallback);
        }
        let stream = build_input_stream_for_input(chain_id, cpal_index, resolved_input, bound)?;
        input_streams.push(stream);
    }

    let mut output_streams = Vec::new();
    for (j, resolved_output) in resolved.outputs.into_iter().enumerate() {
        // LAW (stream isolation): this output mixes ONLY the runtimes whose
        // binding feeds THIS device (by input cpal index) — never all runtimes
        // at its rate (the #743 rate-filter was a leaky proxy that flooded
        // underruns: same-rate runtimes that don't feed this device pop empty).
        let out_slots = crate::slot_processing::slots_for_output_stream(
            &slots,
            &resolved.output_devices_by_input_cpal,
            &resolved_output.device_id,
        );
        // #808: chain output NEVER drains the DI cell — the DI has its OWN
        // isolated stream (invariant #4; the shared cell was the "picotando").
        let di_cell = crate::di_playback::DiPlaybackCell::default();
        let stream =
            build_output_stream_for_output(chain_id, j, resolved_output, out_slots, di_cell)?;
        output_streams.push(stream);
    }

    Ok((input_streams, output_streams))
}

/// Build (and start) the cpal streams for one chain. Issue #350 phase 3:
/// `runtimes` is the chain's full ordered `(group_id, runtime)` list — the
/// cpal path wires each physical input device to its own runtime and the
/// shared output device sums them. The Linux/JACK path is unchanged: it
/// keeps the single-runtime model (Insert / JACK-direct chains are one
/// runtime by Phase-1 design) and uses the first runtime.
pub(crate) fn build_active_chain_runtime(
    chain_id: &ChainId,
    #[allow(unused_variables)] chain: &Chain,
    resolved: ResolvedChainAudioConfig,
    slots: Vec<(usize, LiveRuntimeSlot)>,
    #[allow(unused_variables)] registry: &[IoBinding],
    di_cells: &[crate::di_playback::DiPlaybackCell],
    generation: u64,
) -> Result<ActiveChainRuntime> {
    log::info!(
        "building active chain runtime for '{}', sample_rate={}",
        chain_id.0,
        resolved.sample_rate
    );
    let stream_signature = resolved.stream_signature.clone();
    let structure = crate::io_topology::chain_structure_signature(chain);

    // On Linux with JACK: use the jack crate directly for zero-overhead audio.
    // This bypasses CPAL entirely — the JACK process callback runs in the
    // real-time thread with no extra buffering.
    #[cfg(all(target_os = "linux", feature = "jack"))]
    {
        if jack_server_is_running() {
            log::info!("JACK detected — using direct JACK backend (bypassing CPAL)");
            // JACK-direct chains are a single runtime by Phase-1 design
            // (Insert pipelines are not partitioned). Use the first.
            // JACK does not yet read through the live slot (issue #672 wires the
            // cpal path first); load the published runtime once here so JACK keeps
            // its current behaviour. Live JACK swap is a follow-up.
            let runtime = slots
                .into_iter()
                .next()
                .map(|(_, slot)| slot.load())
                .ok_or_else(|| anyhow::anyhow!("chain '{}' has no runtime state", chain_id.0))?;
            // #771: JACK-direct runs a single output stream — hand it ALL
            // the chain's DI cells so the DI is audible whichever output the
            // arm parked on.
            let (jack_client, dsp_worker) =
                build_jack_direct_chain(chain_id, chain, runtime, registry, di_cells.to_vec())?;
            return Ok(ActiveChainRuntime {
                stream_signature,
                structure,
                generation,
                _input_streams: Vec::new(),
                _output_streams: Vec::new(),
                _jack_client: Some(jack_client),
                _dsp_worker: Some(dsp_worker),
            });
        }
        // JACK not running on Linux+JACK build — return an empty
        // ActiveChainRuntime; resolved.inputs/outputs are empty in this
        // mode. Matches the pre-split behaviour where the function fell
        // through to the CPAL path with nothing to build.
        let _ = chain_id;
        let _ = resolved;
        let _ = slots;
        let _ = di_cells;
        return Ok(ActiveChainRuntime {
            stream_signature,
            structure,
            generation,
            _input_streams: Vec::new(),
            _output_streams: Vec::new(),
            _jack_client: None,
            _dsp_worker: None,
        });
    }

    #[cfg(not(all(target_os = "linux", feature = "jack")))]
    {
        let (input_streams, output_streams) =
            build_chain_streams(chain_id, resolved, slots, di_cells)?;
        for stream in &input_streams {
            stream.play()?;
        }
        for stream in &output_streams {
            stream.play()?;
        }
        log::info!(
            "audio streams started for chain '{}': {} input(s), {} output(s)",
            chain_id.0,
            input_streams.len(),
            output_streams.len()
        );
        Ok(ActiveChainRuntime {
            stream_signature,
            structure,
            generation,
            _input_streams: input_streams,
            _output_streams: output_streams,
        })
    }
}
