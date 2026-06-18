//! SpectrumWindow live session — owns the per-stream stereo sample taps
//! and FFT analyzers that drive the row model.
//!
//! "One spectrum per input" model:
//! - Every Input on every enabled chain is one **stream**
//! - Every stream is internally stereo (mono inputs are upmixed)
//! - Every stream gets **two rows** in the spectrum window: L and R
//! - Two guitars in two chains → 4 rows; two guitars in one DualMono
//!   chain → 4 rows as well
//!
//! The tap point lives inside the engine's `process_single_segment` —
//! after the segment's FX chain has produced the post-effects stereo
//! buffer for that input, and before the buffer is mixed down into the
//! shared output routes. See `engine::stream_tap` for the lock-free SPSC
//! publish contract on the audio thread.
//!
//! The analyzer itself runs entirely on the UI thread; it pulls samples
//! from the SPSC rings on a 33 ms timer and feeds them through a sliding
//! `SpectrumAnalyzer` (75 % overlap, ~23 Hz refresh). Allocation-free
//! steady state: per-row `VecModel<f32>` for levels and peaks is created
//! once at session build and mutated in-place via `set_row_data(i, v)`.

use std::rc::Rc;
use std::sync::Arc;

use engine::spsc::SpscRing;
use feature_dsp::spectrum_fft::{SpectrumAnalyzer, SpectrumSnapshot, FFT_SIZE, N_BANDS};
use infra_cpal::ProjectRuntimeController;
use project::block::AudioBlockKind;
use project::project::Project;
use slint::{Model, ModelRc, VecModel};

#[cfg(test)]
use project::chain::Chain;

use crate::SpectrumRow;

/// Capacity per ring: 4 × FFT_SIZE so a slow UI tick (≈100 ms) can still
/// catch up without dropping samples on a 48 kHz stream.
const RING_CAPACITY: usize = FFT_SIZE * 4;

/// Maximum samples drained per ring per tick — caps the work on the UI
/// thread per timer slot.
const MAX_DRAIN_PER_TICK: usize = FFT_SIZE;

/// One analyzer pipeline for one (stream, channel) pair (L or R).
struct RowState {
    ring: Arc<SpscRing<f32>>,
    analyzer: SpectrumAnalyzer,
    levels_model: Rc<VecModel<f32>>,
    peaks_model: Rc<VecModel<f32>>,
    drain_buf: Vec<f32>,
}

impl RowState {
    fn new(
        ring: Arc<SpscRing<f32>>,
        sample_rate: usize,
        levels_model: Rc<VecModel<f32>>,
        peaks_model: Rc<VecModel<f32>>,
    ) -> Self {
        Self {
            ring,
            analyzer: SpectrumAnalyzer::new(sample_rate as f32),
            levels_model,
            peaks_model,
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

/// Returns the stream index the spectrum should use to subscribe a tap on
/// `chain`. For both binding-based chains (non-empty `io`) and legacy
/// single-entry chains the first (and only) stream always lives at index 0
/// within its `chain_id` key in the runtime graph.
///
/// Returns `None` only for chains with no enabled output block.
///
/// Exposed for tests — pins the session-layer tap-key invariant.
#[cfg(test)]
pub fn tap_stream_index_for_output_chain(chain: &Chain) -> Option<usize> {
    let has_output = chain.blocks.iter().any(|b| {
        matches!(&b.kind, AudioBlockKind::Output(_))
    });
    if has_output { Some(0) } else { None }
}

/// Stable signature of every (chain, stream, channel) the analyzer cares
/// about. Compared at every tick so we can rebuild the session when the
/// user enables/disables chains or edits an InputBlock without forcing a
/// window close. Streams come from the chain's enabled InputBlocks (one
/// stream per InputEntry).
fn project_stream_fingerprint(project: &Project) -> String {
    let mut s = String::new();
    for chain in &project.chains {
        if !chain.enabled {
            continue;
        }
        s.push_str(&chain.id.0);
        s.push('@');
        let mut stream_index = 0_usize;
        for block in &chain.blocks {
            if let AudioBlockKind::Input(input) = &block.kind {
                for entry in &input.entries {
                    s.push_str(&format!(
                        "[{}/{}/{:?}]",
                        stream_index, entry.device_id.0, entry.mode
                    ));
                    stream_index += 1;
                }
            }
        }
        s.push(';');
    }
    s
}

/// Strip the OS backend prefix (`coreaudio:`, `wasapi:`, `jack:`, ...)
/// so the row label shows the device name only. Inner colons preserved.
fn short_device_label(device_id: &str) -> String {
    device_id
        .split_once(':')
        .map(|(_, rest)| rest.to_string())
        .unwrap_or_else(|| device_id.to_string())
}

/// Enumerate the display labels for every active output endpoint in the
/// project — one label per spectrum analyzer row.
///
/// Under the per-binding routing model (Tasks 8/9, #716), each enabled
/// chain's `OutputBlock` carries a non-empty `io` field (binding id) and an
/// `endpoint` field.  Each `(io, endpoint)` pair maps to exactly one runtime
/// output stream and therefore one spectrum analyzer.
///
/// Legacy chains (pre-#716, `io == ""`) continue to enumerate one label per
/// `OutputEntry` so the legacy path is unaffected.
///
/// Exposed for tests — pins the per-binding enumeration contract.
#[cfg(test)]
pub fn output_endpoint_labels_for_project(project: &Project) -> Vec<String> {
    let mut labels = Vec::new();
    for chain in &project.chains {
        if !chain.enabled {
            continue;
        }
        let chain_label = chain
            .description
            .clone()
            .unwrap_or_else(|| chain.id.0.clone());
        for block in &chain.blocks {
            if let AudioBlockKind::Output(output) = &block.kind {
                if !output.io.is_empty() {
                    // Binding-based path: one spectrum row per (io, endpoint).
                    labels.push(format!(
                        "{}  ·  {}  ·  {}",
                        chain_label.to_uppercase(),
                        output.io,
                        output.endpoint,
                    ));
                } else {
                    // Legacy entries path: one label per output entry.
                    for entry in &output.entries {
                        labels.push(format!(
                            "{}  ·  OUT: {}",
                            chain_label.to_uppercase(),
                            short_device_label(&entry.device_id.0),
                        ));
                    }
                }
            }
        }
    }
    labels
}

pub struct SpectrumSession {
    rows_model: Rc<VecModel<SpectrumRow>>,
    row_states: Vec<RowState>,
    fingerprint: String,
}

impl SpectrumSession {
    /// Build a spectrum session for the given project.
    ///
    /// Binding-based chains (#716, `OutputBlock.io` non-empty) each produce
    /// exactly one runtime stream at index 0 within their `chain_id` key.
    /// We subscribe via `subscribe_stream_tap(chain_id, 0)` — the same index
    /// the legacy path uses for its first stream — so every row has live L/R
    /// rings and the spectrum shows real signal.
    ///
    /// Legacy chains (empty `io`) retain the full stream-tap subscribe path:
    /// two rows (L, R) per input stream via `subscribe_stream_tap`.
    ///
    /// Row order and state order are kept in lock-step: every row pushed
    /// into `rows_model` at index `i` has its `RowState` at `row_states[i]`
    /// so `tick()` can use a single index to drive both.
    pub fn build(project: &Project, controller: &ProjectRuntimeController) -> Self {
        let rows_model: Rc<VecModel<SpectrumRow>> =
            Rc::new(VecModel::from(Vec::<SpectrumRow>::new()));
        let mut row_states: Vec<RowState> = Vec::new();

        for chain in &project.chains {
            if !chain.enabled {
                continue;
            }

            let chain_label = chain
                .description
                .clone()
                .unwrap_or_else(|| chain.id.0.clone());

            let has_binding_output = chain.blocks.iter().any(|b| {
                matches!(&b.kind, AudioBlockKind::Output(out) if !out.io.is_empty())
            });

            if has_binding_output {
                // Binding-based path: one spectrum analyzer (L+R rows) per
                // (io, endpoint) pair.  Each binding chain owns exactly one
                // stream at stream_index=0.
                let sample_rate = project
                    .device_settings
                    .iter()
                    .next()
                    .map(|d| d.sample_rate as usize)
                    .unwrap_or(48_000);

                for block in &chain.blocks {
                    if let AudioBlockKind::Output(output) = &block.kind {
                        if output.io.is_empty() {
                            continue;
                        }
                        let row_label = format!(
                            "{}  ·  {}  ·  {}",
                            chain_label.to_uppercase(),
                            output.io,
                            output.endpoint,
                        );

                        let rings =
                            controller.subscribe_stream_tap(&chain.id, 0, RING_CAPACITY);
                        let [l_ring, r_ring] = rings.unwrap_or_else(|| {
                            [
                                Arc::new(SpscRing::new(RING_CAPACITY, 0.0_f32)),
                                Arc::new(SpscRing::new(RING_CAPACITY, 0.0_f32)),
                            ]
                        });

                        // L row
                        let l_levels = make_zero_band_model();
                        let l_peaks = make_zero_band_model();
                        rows_model.push(SpectrumRow {
                            label: format!("{} · L", row_label).into(),
                            levels: ModelRc::from(l_levels.clone()),
                            peaks: ModelRc::from(l_peaks.clone()),
                            active: false,
                        });
                        row_states.push(RowState::new(l_ring, sample_rate, l_levels, l_peaks));

                        // R row
                        let r_levels = make_zero_band_model();
                        let r_peaks = make_zero_band_model();
                        rows_model.push(SpectrumRow {
                            label: format!("{} · R", row_label).into(),
                            levels: ModelRc::from(r_levels.clone()),
                            peaks: ModelRc::from(r_peaks.clone()),
                            active: false,
                        });
                        row_states.push(RowState::new(r_ring, sample_rate, r_levels, r_peaks));
                    }
                }
            } else {
                // Legacy path: two rows (L, R) per input stream.

                // Engine-side stream count. The engine `effective_inputs`
                // expansion can split a single mono multi-channel `InputEntry`
                // into several streams (one per channel), so iterating
                // `chain.blocks` would under-count. We ask the runtime
                // directly so the subscribe loop is always aligned with the
                // engine's `seg_idx` space.
                let stream_count = controller.stream_count(&chain.id);
                log::info!(
                    "spectrum_session: chain '{}' has {} streams",
                    chain.id.0,
                    stream_count
                );

                // Best-effort device label for each stream — picks the input
                // entries in declaration order, falling back to the chain id
                // if there are more streams than entries (e.g. mono splits).
                let mut entry_labels: Vec<String> = Vec::new();
                for block in &chain.blocks {
                    if let AudioBlockKind::Input(input) = &block.kind {
                        for entry in &input.entries {
                            let label = short_device_label(&entry.device_id.0);
                            if matches!(entry.mode, project::chain::ChainInputMode::Mono)
                                && entry.channels.len() > 1
                            {
                                // The engine splits this mono entry into one
                                // stream per channel — produce a per-channel
                                // label so the spectrum rows stay readable.
                                for &ch in &entry.channels {
                                    entry_labels.push(format!("{label} CH {}", ch + 1));
                                }
                            } else {
                                entry_labels.push(label);
                            }
                        }
                    }
                }

                let sample_rate = chain
                    .blocks
                    .iter()
                    .find_map(|b| match &b.kind {
                        AudioBlockKind::Input(input) => input.entries.first().and_then(|entry| {
                            project
                                .device_settings
                                .iter()
                                .find(|d| d.device_id == entry.device_id)
                                .map(|d| d.sample_rate as usize)
                        }),
                        _ => None,
                    })
                    .unwrap_or(48_000);

                for stream_index in 0..stream_count {
                    let device_label = entry_labels
                        .get(stream_index)
                        .cloned()
                        .unwrap_or_else(|| format!("stream {}", stream_index + 1));

                    let rings =
                        controller.subscribe_stream_tap(&chain.id, stream_index, RING_CAPACITY);
                    let Some([l_ring, r_ring]) = rings else {
                        log::warn!(
                            "spectrum_session: subscribe_stream_tap returned None for chain '{}' stream {}",
                            chain.id.0,
                            stream_index
                        );
                        continue;
                    };

                    // L row
                    let l_levels = make_zero_band_model();
                    let l_peaks = make_zero_band_model();
                    rows_model.push(SpectrumRow {
                        label: format!(
                            "{}  ·  IN: {}  ·  L",
                            chain_label.to_uppercase(),
                            device_label
                        )
                        .into(),
                        levels: ModelRc::from(l_levels.clone()),
                        peaks: ModelRc::from(l_peaks.clone()),
                        active: false,
                    });
                    row_states.push(RowState::new(l_ring, sample_rate, l_levels, l_peaks));

                    // R row
                    let r_levels = make_zero_band_model();
                    let r_peaks = make_zero_band_model();
                    rows_model.push(SpectrumRow {
                        label: format!(
                            "{}  ·  IN: {}  ·  R",
                            chain_label.to_uppercase(),
                            device_label
                        )
                        .into(),
                        levels: ModelRc::from(r_levels.clone()),
                        peaks: ModelRc::from(r_peaks.clone()),
                        active: false,
                    });
                    row_states.push(RowState::new(r_ring, sample_rate, r_levels, r_peaks));
                }
            }
        }

        Self {
            rows_model,
            row_states,
            fingerprint: project_stream_fingerprint(project),
        }
    }

    pub fn rows_model_rc(&self) -> ModelRc<SpectrumRow> {
        ModelRc::from(self.rows_model.clone())
    }

    pub fn needs_rebuild(&self, project: &Project) -> bool {
        self.fingerprint != project_stream_fingerprint(project)
    }

    /// Drain rings, feed the analyzer's sliding window, update the row
    /// model in-place. Allocation-free on the steady state.
    pub fn tick(&mut self) {
        for (idx, state) in self.row_states.iter_mut().enumerate() {
            state.drain_buf.clear();
            for _ in 0..MAX_DRAIN_PER_TICK {
                match state.ring.pop() {
                    Some(s) => state.drain_buf.push(s),
                    None => break,
                }
            }
            if state.drain_buf.is_empty() {
                continue;
            }
            let drain_slice: &[f32] = &state.drain_buf;
            if let Some(snap) = state.analyzer.process_chunk(drain_slice) {
                write_snapshot_into(&snap, &state.levels_model, &state.peaks_model);
                let active = snap.peaks.iter().any(|&p| p > 0.05);
                if let Some(mut row) = self.rows_model.row_data(idx) {
                    if row.active != active {
                        row.active = active;
                        self.rows_model.set_row_data(idx, row);
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
        for (idx, state) in self.row_states.iter_mut().enumerate() {
            reset_band_model(&state.levels_model);
            reset_band_model(&state.peaks_model);
            if let Some(mut row) = self.rows_model.row_data(idx) {
                if row.active {
                    row.active = false;
                    self.rows_model.set_row_data(idx, row);
                }
            }
        }
    }
}

#[cfg(test)]
#[path = "spectrum_session_tests.rs"]
mod tests;
