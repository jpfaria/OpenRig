//! Responsibility: owns the live spectrum session.
//! SpectrumWindow live session — owns the per-stream stereo sample taps
//! and FFT analyzers that drive the row model.
//!
//! "One spectrum per input" model:
//! - Every Input on every enabled chain is one **stream**
//! - Every stream is internally stereo (mono inputs are upmixed)
//! - Every stream gets **two rows** (L and R) per output it feeds
//! - Two guitars with three live outputs each → 12 rows; an unchecked
//!   input or output has no rows
//!
//! The tap point lives inside the engine's `process_single_segment` —
//! after the segment's FX chain has produced the post-effects stereo
//! buffer for that input, and before the buffer is mixed down into the
//! shared output routes. See `engine::stream_tap` for the lock-free SPSC
//! publish contract on the audio thread.
//!
//! The analyzer itself runs entirely on the UI thread; it pulls samples
//! from its subscription (`application::audio_taps`) on a 33 ms timer and
//! feeds them through a sliding
//! `SpectrumAnalyzer` (75 % overlap, ~23 Hz refresh). Allocation-free
//! steady state: per-row `VecModel<f32>` for levels and peaks is created
//! once at session build and mutated in-place via `set_row_data(i, v)`.

use std::rc::Rc;
use std::sync::Arc;

use application::audio_taps::{AudioTap, AudioTaps, TapPoint};
use domain::io_binding::IoBinding;
use feature_dsp::spectrum_fft::{SpectrumAnalyzer, SpectrumSnapshot, FFT_SIZE, N_BANDS};
use project::project::Project;
use slint::{Model, ModelRc, VecModel};

use crate::spectrum_row_label::{spectrum_output_labels, spectrum_row_label};
use crate::SpectrumRow;

/// Capacity per channel: 4 × FFT_SIZE so a slow UI tick (≈100 ms) can still
/// catch up without dropping samples on a 48 kHz stream.
const RING_CAPACITY: usize = FFT_SIZE * 4;

/// Maximum samples drained per channel per tick — caps the work on the UI
/// thread per timer slot.
const MAX_DRAIN_PER_TICK: usize = FFT_SIZE;

/// One analyzer pipeline for one (stream, channel) pair (L or R).
///
/// The L and R rows of one stream SHARE the stream's single stereo
/// subscription and address their own channel — a tap per row would be two
/// taps on the audio callback where the engine needs one. A stream feeding
/// several outputs shows one row per output on each side; those rows share
/// this pipeline, since draining the tap once per row would split the signal.
struct RowState {
    tap: Arc<dyn AudioTap>,
    channel: usize,
    analyzer: SpectrumAnalyzer,
    rows: Vec<RowView>,
    drain_buf: Vec<f32>,
}

/// One rendered row a pipeline writes into.
struct RowView {
    index: usize,
    levels_model: Rc<VecModel<f32>>,
    peaks_model: Rc<VecModel<f32>>,
}

impl RowState {
    fn new(tap: Arc<dyn AudioTap>, channel: usize, sample_rate: usize) -> Self {
        Self {
            tap,
            channel,
            analyzer: SpectrumAnalyzer::new(sample_rate as f32),
            rows: Vec::new(),
            drain_buf: Vec::with_capacity(MAX_DRAIN_PER_TICK),
        }
    }
}

fn make_zero_band_model() -> Rc<VecModel<f32>> {
    Rc::new(VecModel::from(vec![0.0_f32; N_BANDS]))
}

fn write_snapshot_into(
    snap: &SpectrumSnapshot,
    levels_model: &Rc<VecModel<f32>>,
    peaks_model: &Rc<VecModel<f32>>,
) {
    for i in 0..N_BANDS {
        levels_model.set_row_data(i, snap.levels[i]);
        peaks_model.set_row_data(i, snap.peaks[i]);
    }
}

fn reset_band_model(model: &Rc<VecModel<f32>>) {
    for i in 0..N_BANDS {
        model.set_row_data(i, 0.0);
    }
}

/// Stable signature of every (chain, stream, channel) the analyzer cares
/// about. Compared at every tick so we can rebuild the session when the
/// user enables/disables chains or edits an InputBlock without forcing a
/// window close. Streams come from the chain's enabled InputBlocks (one
/// stream per InputEntry).
fn project_stream_fingerprint(project: &Project, registry: &[IoBinding]) -> String {
    let mut s = String::new();
    for chain in &project.chains {
        if !chain.enabled {
            continue;
        }
        s.push_str(&chain.id.0);
        s.push('@');
        // #716: device endpoints resolve from the binding registry, not from
        // block `entries`.
        let (resolved_inputs, _) = engine::runtime_endpoints::resolve_chain_io(chain, registry);
        for (stream_index, entry) in resolved_inputs.iter().enumerate() {
            s.push_str(&format!(
                "[{}/{}/{:?}]",
                stream_index, entry.device_id.0, entry.mode
            ));
        }
        // The rows follow the outputs each stream feeds, so a checked or
        // unchecked output rebuilds the session too.
        for stream in engine::stream_io_labels::chain_stream_io_labels(chain, registry) {
            for output in &stream.outputs {
                s.push_str(&format!("<{}/{}>", output.device, output.channels));
            }
            s.push('|');
        }
        s.push(';');
    }
    s
}

/// Which tap a spectrum row reads. The Slint row carries only a display
/// label; other transports (#829) need the structured identity. `channel`
/// is the row's side of the stereo stream tap (0 = L, 1 = R).
#[derive(Debug, Clone, PartialEq)]
pub struct RowIdentity {
    pub chain: String,
    pub input: usize,
    pub channel: usize,
}

/// Pair each row's live band values with its tap identity, in the shape
/// every transport reads (`openrig://spectrum`).
pub fn readings_from(
    identities: &[RowIdentity],
    rows: &VecModel<SpectrumRow>,
) -> Vec<application::query_analyzers::SpectrumReading> {
    identities
        .iter()
        .enumerate()
        .filter_map(|(idx, id)| rows.row_data(idx).map(|row| (id, row)))
        .map(|(id, row)| application::query_analyzers::SpectrumReading {
            chain: id.chain.clone(),
            input: id.input,
            channel: id.channel,
            label: row.label.to_string(),
            levels: row.levels.iter().collect(),
            peaks: row.peaks.iter().collect(),
            active: row.active,
        })
        .collect()
}

pub struct SpectrumSession {
    rows_model: Rc<VecModel<SpectrumRow>>,
    row_states: Vec<RowState>,
    identities: Vec<RowIdentity>,
    fingerprint: String,
}

impl SpectrumSession {
    /// Build a spectrum session for the given project: subscribe one
    /// stereo stream tap per engine stream of every enabled chain, and
    /// create two rows (L, R) for each output that stream feeds.
    pub fn build(project: &Project, taps: &dyn AudioTaps, registry: &[IoBinding]) -> Self {
        let rows_model: Rc<VecModel<SpectrumRow>> =
            Rc::new(VecModel::from(Vec::<SpectrumRow>::new()));
        let mut row_states: Vec<RowState> = Vec::new();
        let mut identities: Vec<RowIdentity> = Vec::new();

        // The rate the live streams actually run at — authoritative fallback
        // for inputs without a saved per-device setting (issue #723).
        let live_sample_rate = taps.live_sample_rate();

        for chain in &project.chains {
            if !chain.enabled {
                continue;
            }

            let chain_label = chain
                .description
                .clone()
                .unwrap_or_else(|| chain.id.0.clone());

            // Engine-side stream count. The engine `effective_inputs`
            // expansion can split a single mono multi-channel `InputEntry`
            // into several streams (one per channel), so iterating
            // `chain.blocks` would under-count. We ask the runtime
            // directly so the subscribe loop is always aligned with the
            // engine's `seg_idx` space.
            let stream_count = taps.stream_count(&chain.id);
            log::info!(
                "spectrum_session: chain '{}' has {} streams",
                chain.id.0,
                stream_count
            );

            // #716: the chain's input endpoints resolve from the binding
            // registry, not from block `entries`.
            let (resolved_inputs, _) = engine::runtime_endpoints::resolve_chain_io(chain, registry);

            // The meters' labels: one per engine stream, read off the same
            // segment map the runtime counts its streams from, each side
            // named after the interface as the host names it.
            let stream_labels = crate::meter_row_labels::project_stream_labels(
                chain,
                registry,
                &crate::device_refresh_list::cached_devices(),
            );

            let sample_rate = resolved_inputs
                .first()
                .map(|entry| {
                    crate::sample_rate::resolve_input_sample_rate(
                        project,
                        &entry.device_id,
                        live_sample_rate,
                    )
                })
                .unwrap_or(live_sample_rate as usize);

            for stream_index in 0..stream_count {
                let Some(tap) = taps.subscribe(
                    &TapPoint::StreamOutput {
                        chain: chain.id.clone(),
                        stream: stream_index,
                    },
                    RING_CAPACITY,
                ) else {
                    log::warn!(
                        "spectrum_session: no stream-output tap for chain '{}' stream {}",
                        chain.id.0,
                        stream_index
                    );
                    continue;
                };

                // One L/R pair per output the stream feeds; every pair reads
                // the stream's single tap — the stream writes the same signal
                // to each of its outputs.
                let mut sides = [
                    RowState::new(Arc::clone(&tap), 0, sample_rate),
                    RowState::new(tap, 1, sample_rate),
                ];
                let outputs = spectrum_output_labels(
                    &chain_label,
                    stream_labels.get(stream_index),
                    stream_index,
                );
                for output_label in outputs {
                    for (channel, state) in sides.iter_mut().enumerate() {
                        let side = if channel == 0 { "L" } else { "R" };
                        let levels = make_zero_band_model();
                        let peaks = make_zero_band_model();
                        state.rows.push(RowView {
                            index: rows_model.row_count(),
                            levels_model: levels.clone(),
                            peaks_model: peaks.clone(),
                        });
                        rows_model.push(SpectrumRow {
                            label: spectrum_row_label(&output_label, side).into(),
                            output: output_label.as_str().into(),
                            levels: ModelRc::from(levels),
                            peaks: ModelRc::from(peaks),
                            active: false,
                        });
                        identities.push(RowIdentity {
                            chain: chain.id.0.clone(),
                            input: stream_index,
                            channel,
                        });
                    }
                }
                row_states.extend(sides);
            }
        }

        Self {
            rows_model,
            row_states,
            identities,
            fingerprint: project_stream_fingerprint(project, registry),
        }
    }

    pub fn rows_model_rc(&self) -> ModelRc<SpectrumRow> {
        ModelRc::from(self.rows_model.clone())
    }

    /// #829: the live readings as every transport reads them
    /// (`openrig://spectrum`) — same rows the window renders.
    pub fn readings(&self) -> Vec<application::query_analyzers::SpectrumReading> {
        readings_from(&self.identities, &self.rows_model)
    }

    pub fn needs_rebuild(&self, project: &Project, registry: &[IoBinding]) -> bool {
        self.fingerprint != project_stream_fingerprint(project, registry)
    }

    /// Drain the subscriptions, feed the analyzer's sliding window, update the row
    /// model in-place. Allocation-free on the steady state.
    pub fn tick(&mut self) {
        for state in self.row_states.iter_mut() {
            state.drain_buf.clear();
            state
                .tap
                .drain_channel(state.channel, MAX_DRAIN_PER_TICK, &mut state.drain_buf);
            if state.drain_buf.is_empty() {
                continue;
            }
            let drain_slice: &[f32] = &state.drain_buf;
            if let Some(snap) = state.analyzer.process_chunk(drain_slice) {
                let active = snap.peaks.iter().any(|&p| p > 0.05);
                for view in &state.rows {
                    write_snapshot_into(&snap, &view.levels_model, &view.peaks_model);
                    if let Some(mut row) = self.rows_model.row_data(view.index) {
                        if row.active != active {
                            row.active = active;
                            self.rows_model.set_row_data(view.index, row);
                        }
                    }
                }
            }
        }
    }

    /// Clear every row's bars + peaks + active flag without dropping the
    /// session. Use this when the project's stream topology is gone (last
    /// chain disabled, runtime torn down) so the window does not show
    /// stale bars frozen from the last live frame.
    pub fn freeze_to_zero(&mut self) {
        for view in self.row_states.iter().flat_map(|state| &state.rows) {
            reset_band_model(&view.levels_model);
            reset_band_model(&view.peaks_model);
            if let Some(mut row) = self.rows_model.row_data(view.index) {
                if row.active {
                    row.active = false;
                    self.rows_model.set_row_data(view.index, row);
                }
            }
        }
    }
}

#[cfg(test)]
#[path = "spectrum_session_tests.rs"]
mod tests;

#[cfg(test)]
#[path = "spectrum_session_readings_tests.rs"]
mod readings_tests;

#[cfg(test)]
#[path = "spectrum_session_outputs_tests.rs"]
mod outputs_tests;
