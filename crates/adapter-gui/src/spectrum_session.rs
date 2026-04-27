//! SpectrumWindow live session — owns the per-row sample taps and FFT
//! analyzers that drive the row model.
//!
//! Mirror of `tuner_session.rs` but on the **output** side: subscribes one
//! ring per channel of every terminal Output entry of every enabled chain.
//! The audio thread pushes post-FX samples into the rings (see
//! `engine::output_tap`); on a UI timer the session drains each ring,
//! feeds the samples to a [`SpectrumAnalyzer`] which keeps a sliding
//! history (75 % overlap, ~23 Hz refresh @ 48 kHz) and emits a
//! [`SpectrumSnapshot`] every `HOP_SIZE` samples. The snapshot is mapped
//! onto pre-allocated `VecModel<f32>` rows so no allocation happens per
//! tick.
//!
//! Audio thread is untouched — the analyzer runs entirely on the UI
//! thread, and the tap publish path remains the same lock-free SPSC push
//! introduced in #320 Phase 2.

use std::rc::Rc;
use std::sync::Arc;

use engine::spsc::SpscRing;
use feature_dsp::spectrum_fft::{SpectrumAnalyzer, SpectrumSnapshot, N_BANDS};
use infra_cpal::ProjectRuntimeController;
use project::block::AudioBlockKind;
use project::project::Project;
use slint::{Model, ModelRc, VecModel};

use crate::SpectrumRow;

/// Capacity per channel ring: 4 × FFT_SIZE so a slow UI tick (≈100 ms)
/// can still catch up without dropping samples on a 48 kHz stream.
const RING_CAPACITY: usize = feature_dsp::spectrum_fft::FFT_SIZE * 4;

/// Maximum samples drained per ring per tick. Caps the work the UI thread
/// does in a single 33 ms slot — even if the ring backed up, we will not
/// stall the timer trying to catch up in one go.
const MAX_DRAIN_PER_TICK: usize = feature_dsp::spectrum_fft::FFT_SIZE;

/// One analyzer pipeline per (chain, output, channel).
struct RowState {
    ring: Arc<SpscRing<f32>>,
    analyzer: SpectrumAnalyzer,
    /// Reusable Slint model holding the 63 band levels. Created once at
    /// session build, mutated in-place on every tick — no allocation per
    /// FFT.
    levels_model: Rc<VecModel<f32>>,
    peaks_model: Rc<VecModel<f32>>,
    /// Pre-allocated drain scratch — fills from `ring.pop()` in `tick`,
    /// fed to `analyzer.process_chunk` in one call. Avoids per-tick
    /// `Vec::new` and gives the analyzer its full view of incoming
    /// samples in chronological order.
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

#[cfg(test)]
fn empty_row(label: String) -> SpectrumRow {
    SpectrumRow {
        label: label.into(),
        levels: ModelRc::from(make_zero_band_model()),
        peaks: ModelRc::from(make_zero_band_model()),
        active: false,
    }
}

/// Stable signature of every (chain, output, channel) the analyzer cares
/// about. Compared at every tick so we can rebuild the session when the
/// user enables/disables chains or edits an OutputBlock without forcing a
/// window close.
fn project_output_fingerprint(project: &Project) -> String {
    let mut s = String::new();
    for chain in &project.chains {
        if !chain.enabled {
            continue;
        }
        s.push_str(&chain.id.0);
        s.push('@');
        let mut output_index = 0_usize;
        for block in &chain.blocks {
            if let AudioBlockKind::Output(output) = &block.kind {
                for entry in &output.entries {
                    s.push_str(&format!("[{}/{}/", output_index, entry.device_id.0));
                    for ch in &entry.channels {
                        s.push_str(&format!("{},", ch));
                    }
                    s.push_str(&format!("/{:?}]", entry.mode));
                    output_index += 1;
                }
            }
        }
        s.push(';');
    }
    s
}

/// Short device label for the row title. Strips the OS-specific backend
/// prefix (`coreaudio:`, `wasapi:`, `jack:`, ...) so the user just sees
/// the device name. The "· CH N" suffix is rendered separately by the
/// caller, so any remaining colons inside the id are preserved as-is.
fn short_device_label(device_id: &str) -> String {
    device_id
        .split_once(':')
        .map(|(_, rest)| rest.to_string())
        .unwrap_or_else(|| device_id.to_string())
}

/// First enabled-input device label for a chain. The spectrum tap is on
/// the chain's *output*, but the user reads the row as "this chain's
/// signal coming out of <device>", so we surface the input source so the
/// row is identifiable when several chains share a device.
fn chain_input_label(chain: &project::chain::Chain) -> Option<String> {
    for block in &chain.blocks {
        if let AudioBlockKind::Input(input) = &block.kind {
            if let Some(entry) = input.entries.first() {
                return Some(short_device_label(&entry.device_id.0));
            }
        }
    }
    None
}

pub struct SpectrumSession {
    rows_model: Rc<VecModel<SpectrumRow>>,
    row_states: Vec<RowState>,
    fingerprint: String,
}

impl SpectrumSession {
    /// Build a spectrum session for the given project: subscribe taps for
    /// every active output channel of every enabled chain. Each row owns
    /// a pre-allocated `VecModel<f32>` for its 63 levels and another for
    /// peaks, so the per-tick update path is allocation-free.
    pub fn build(project: &Project, controller: &ProjectRuntimeController) -> Self {
        let rows_model: Rc<VecModel<SpectrumRow>> =
            Rc::new(VecModel::from(Vec::<SpectrumRow>::new()));
        let mut row_states: Vec<RowState> = Vec::new();

        for chain in &project.chains {
            if !chain.enabled {
                continue;
            }

            let sample_rate = chain
                .blocks
                .iter()
                .find_map(|b| match &b.kind {
                    AudioBlockKind::Output(output) => output.entries.first().and_then(|entry| {
                        project
                            .device_settings
                            .iter()
                            .find(|d| d.device_id == entry.device_id)
                            .map(|d| d.sample_rate)
                    }),
                    _ => None,
                })
                .unwrap_or(48_000) as usize;

            let chain_label = chain
                .description
                .clone()
                .unwrap_or_else(|| chain.id.0.clone());
            let input_hint = chain_input_label(chain);

            let mut output_index = 0_usize;
            for block in &chain.blocks {
                if let AudioBlockKind::Output(output) = &block.kind {
                    for entry in &output.entries {
                        if entry.channels.is_empty() {
                            output_index += 1;
                            continue;
                        }
                        let max_channel = *entry.channels.iter().max().unwrap_or(&0);
                        let total_channels = max_channel + 1;

                        let rings = controller.subscribe_output_tap(
                            &chain.id,
                            output_index,
                            total_channels,
                            &entry.channels,
                            RING_CAPACITY,
                        );

                        let device_label = short_device_label(&entry.device_id.0);

                        for (ch_pos, (channel, ring)) in
                            entry.channels.iter().zip(rings.into_iter()).enumerate()
                        {
                            let ch_label = if entry.channels.len() == 1 {
                                String::new()
                            } else if ch_pos == 0 {
                                " · L".to_string()
                            } else if ch_pos == 1 {
                                " · R".to_string()
                            } else {
                                format!(" · ch{}", ch_pos + 1)
                            };
                            let label = match input_hint.as_ref() {
                                Some(hint) => format!(
                                    "{}  ·  IN: {}  →  OUT: {} · CH {}{}",
                                    chain_label.to_uppercase(),
                                    hint,
                                    device_label,
                                    channel + 1,
                                    ch_label
                                ),
                                None => format!(
                                    "{}  ·  OUT: {} · CH {}{}",
                                    chain_label.to_uppercase(),
                                    device_label,
                                    channel + 1,
                                    ch_label
                                ),
                            };

                            let levels_model = make_zero_band_model();
                            let peaks_model = make_zero_band_model();
                            rows_model.push(SpectrumRow {
                                label: label.into(),
                                levels: ModelRc::from(levels_model.clone()),
                                peaks: ModelRc::from(peaks_model.clone()),
                                active: false,
                            });
                            row_states.push(RowState::new(
                                ring,
                                sample_rate,
                                levels_model,
                                peaks_model,
                            ));
                        }

                        output_index += 1;
                    }
                }
            }
        }

        Self {
            rows_model,
            row_states,
            fingerprint: project_output_fingerprint(project),
        }
    }

    pub fn rows_model_rc(&self) -> ModelRc<SpectrumRow> {
        ModelRc::from(self.rows_model.clone())
    }

    /// Cheap re-fingerprint check. Returns `true` when the output topology
    /// changed since this session was built.
    pub fn needs_rebuild(&self, project: &Project) -> bool {
        self.fingerprint != project_output_fingerprint(project)
    }

    /// Drain rings, feed the analyzer's sliding window, update the row
    /// model in-place. Allocation-free on the steady state.
    pub fn tick(&mut self) {
        for (idx, state) in self.row_states.iter_mut().enumerate() {
            // Drain into a pre-allocated scratch buffer (capped per tick).
            state.drain_buf.clear();
            for _ in 0..MAX_DRAIN_PER_TICK {
                match state.ring.pop() {
                    Some(s) => state.drain_buf.push(s),
                    None => break,
                }
            }

            let drained = state.drain_buf.len();
            if drained == 0 {
                continue;
            }

            // process_chunk runs as many FFTs as HOP_SIZE boundaries were
            // crossed, returning only the latest snapshot — that is what
            // the UI shows anyway.
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
    /// session. Use this when the project's output topology is gone (last
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
    use project::block::{AudioBlock, AudioBlockKind, OutputBlock, OutputEntry};
    use project::chain::{Chain, ChainOutputMode};
    use project::device::DeviceSettings;

    fn output_entry(device: &str, channels: Vec<usize>, mode: ChainOutputMode) -> OutputEntry {
        OutputEntry {
            device_id: DeviceId(device.into()),
            mode,
            channels,
        }
    }

    fn output_block(entries: Vec<OutputEntry>) -> AudioBlock {
        AudioBlock {
            id: BlockId("chain:0:out".into()),
            enabled: true,
            kind: AudioBlockKind::Output(OutputBlock {
                model: "standard".into(),
                entries,
            }),
        }
    }

    fn chain_with_output(id: &str, enabled: bool, entries: Vec<OutputEntry>) -> Chain {
        Chain {
            id: ChainId(id.into()),
            description: Some("Guitar".into()),
            instrument: "electric_guitar".to_string(),
            enabled,
            blocks: vec![output_block(entries)],
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
    fn empty_row_starts_inactive_with_zero_arrays() {
        let row = empty_row("CHAIN · OUT 1".into());
        assert_eq!(row.label, "CHAIN · OUT 1");
        assert!(!row.active);
        assert_eq!(row.levels.iter().count(), N_BANDS);
        assert_eq!(row.peaks.iter().count(), N_BANDS);
    }

    #[test]
    fn fingerprint_skips_disabled_chains() {
        let entries = vec![output_entry("dev:1", vec![0, 1], ChainOutputMode::Stereo)];
        let fp_enabled = project_output_fingerprint(&project_from_chain(chain_with_output(
            "chain:0",
            true,
            entries.clone(),
        )));
        let fp_disabled = project_output_fingerprint(&project_from_chain(chain_with_output(
            "chain:0",
            false,
            entries,
        )));
        assert_ne!(fp_enabled, fp_disabled);
        assert!(fp_disabled.is_empty());
    }

    #[test]
    fn fingerprint_changes_when_channels_change() {
        let mono = vec![output_entry("dev:1", vec![0], ChainOutputMode::Mono)];
        let stereo = vec![output_entry("dev:1", vec![0, 1], ChainOutputMode::Stereo)];

        let fp_mono = project_output_fingerprint(&project_from_chain(chain_with_output(
            "chain:0",
            true,
            mono,
        )));
        let fp_stereo = project_output_fingerprint(&project_from_chain(chain_with_output(
            "chain:0",
            true,
            stereo,
        )));
        assert_ne!(fp_mono, fp_stereo);
    }

    #[test]
    fn fingerprint_stable_for_identical_projects() {
        let mk = || {
            project_from_chain(chain_with_output(
                "chain:0",
                true,
                vec![output_entry("dev:1", vec![0, 1], ChainOutputMode::Stereo)],
            ))
        };
        assert_eq!(
            project_output_fingerprint(&mk()),
            project_output_fingerprint(&mk())
        );
    }

    #[test]
    fn short_device_label_strips_backend_prefix() {
        assert_eq!(short_device_label("coreaudio:Built-in Output"), "Built-in Output");
        // Inner colons are preserved — only the leading backend prefix is stripped.
        assert_eq!(short_device_label("jack:system:playback_1"), "system:playback_1");
        // No prefix → returned as-is.
        assert_eq!(short_device_label("plain-device"), "plain-device");
    }
}
