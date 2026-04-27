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

pub struct SpectrumSession {
    rows_model: Rc<VecModel<SpectrumRow>>,
    row_states: Vec<RowState>,
    fingerprint: String,
}

impl SpectrumSession {
    /// Build a spectrum session for the given project: subscribe one
    /// stereo stream tap per InputEntry of every enabled chain, and
    /// create two rows (L, R) for each tap.
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
                        if matches!(
                            entry.mode,
                            project::chain::ChainInputMode::Mono
                        ) && entry.channels.len() > 1
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

                let rings = controller
                    .subscribe_stream_tap(&chain.id, stream_index, RING_CAPACITY);
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
mod tests {
    use super::*;
    use domain::ids::{BlockId, ChainId, DeviceId};
    use project::block::{AudioBlock, AudioBlockKind, InputBlock, InputEntry};
    use project::chain::{Chain, ChainInputMode};
    use project::device::DeviceSettings;

    fn input_entry(device: &str, channels: Vec<usize>, mode: ChainInputMode) -> InputEntry {
        InputEntry {
            device_id: DeviceId(device.into()),
            mode,
            channels,
        }
    }

    fn input_block(entries: Vec<InputEntry>) -> AudioBlock {
        AudioBlock {
            id: BlockId("chain:0:in".into()),
            enabled: true,
            kind: AudioBlockKind::Input(InputBlock {
                model: "standard".into(),
                entries,
            }),
        }
    }

    fn chain_with_input(id: &str, enabled: bool, entries: Vec<InputEntry>) -> Chain {
        Chain {
            id: ChainId(id.into()),
            description: Some("Guitar".into()),
            instrument: "electric_guitar".to_string(),
            enabled,
            blocks: vec![input_block(entries)],
        }
    }

    fn project_from_chain(chain: Chain) -> Project {
        Project {
            name: None,
            device_settings: Vec::<DeviceSettings>::new(),
            chains: vec![chain],
        }
    }

    #[test]
    fn fingerprint_skips_disabled_chains() {
        let entries = vec![input_entry("dev:1", vec![0], ChainInputMode::Mono)];
        let fp_enabled = project_stream_fingerprint(&project_from_chain(chain_with_input(
            "chain:0",
            true,
            entries.clone(),
        )));
        let fp_disabled = project_stream_fingerprint(&project_from_chain(chain_with_input(
            "chain:0",
            false,
            entries,
        )));
        assert_ne!(fp_enabled, fp_disabled);
        assert!(fp_disabled.is_empty());
    }

    #[test]
    fn fingerprint_changes_when_input_mode_changes() {
        let mono = vec![input_entry("dev:1", vec![0], ChainInputMode::Mono)];
        let stereo = vec![input_entry("dev:1", vec![0, 1], ChainInputMode::Stereo)];

        let fp_mono = project_stream_fingerprint(&project_from_chain(chain_with_input(
            "chain:0",
            true,
            mono,
        )));
        let fp_stereo = project_stream_fingerprint(&project_from_chain(chain_with_input(
            "chain:0",
            true,
            stereo,
        )));
        assert_ne!(fp_mono, fp_stereo);
    }

    #[test]
    fn fingerprint_changes_when_device_id_changes() {
        let dev_a = vec![input_entry("dev:1", vec![0], ChainInputMode::Mono)];
        let dev_b = vec![input_entry("dev:2", vec![0], ChainInputMode::Mono)];

        let fp_a = project_stream_fingerprint(&project_from_chain(chain_with_input(
            "chain:0",
            true,
            dev_a,
        )));
        let fp_b = project_stream_fingerprint(&project_from_chain(chain_with_input(
            "chain:0",
            true,
            dev_b,
        )));
        assert_ne!(fp_a, fp_b);
    }

    #[test]
    fn fingerprint_stable_for_identical_projects() {
        let mk = || {
            project_from_chain(chain_with_input(
                "chain:0",
                true,
                vec![input_entry("dev:1", vec![0], ChainInputMode::Mono)],
            ))
        };
        assert_eq!(
            project_stream_fingerprint(&mk()),
            project_stream_fingerprint(&mk())
        );
    }

    #[test]
    fn short_device_label_strips_backend_prefix() {
        assert_eq!(short_device_label("coreaudio:Built-in Output"), "Built-in Output");
        assert_eq!(short_device_label("jack:system:playback_1"), "system:playback_1");
        assert_eq!(short_device_label("plain-device"), "plain-device");
    }
}
