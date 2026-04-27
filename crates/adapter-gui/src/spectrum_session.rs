//! SpectrumWindow live session — owns the per-row sample taps, accumulators
//! and FFT analyzers that drive the row model.
//!
//! Mirror of `tuner_session.rs` but on the **output** side: subscribes one
//! ring per channel of every terminal Output entry of every enabled chain.
//! The audio thread pushes post-FX samples into the rings (see
//! `engine::output_tap`); on a UI timer the session drains each ring into a
//! short accumulator and, once `FFT_SIZE` samples are queued, dispatches a
//! [`SpectrumAnalyzer::process`] call. The resulting [`SpectrumSnapshot`] is
//! mapped onto the Slint row model as two `[f32; N_BANDS]` arrays.

use std::rc::Rc;
use std::sync::Arc;

use engine::spsc::SpscRing;
use feature_dsp::spectrum_fft::{SpectrumAnalyzer, SpectrumSnapshot, FFT_SIZE, N_BANDS};
use infra_cpal::ProjectRuntimeController;
use project::block::AudioBlockKind;
use project::project::Project;
use slint::{Model, ModelRc, VecModel};

use crate::SpectrumRow;

/// Capacity per channel ring: ≥ FFT_SIZE × 2 so we never lose samples
/// between UI ticks under any reasonable timer cadence (~30 Hz).
const RING_CAPACITY: usize = FFT_SIZE * 2;

/// One analyzer pipeline per (chain, output, channel).
struct RowState {
    ring: Arc<SpscRing<f32>>,
    sample_buf: Vec<f32>,
    analyzer: SpectrumAnalyzer,
}

impl RowState {
    fn new(ring: Arc<SpscRing<f32>>, sample_rate: usize) -> Self {
        Self {
            ring,
            sample_buf: Vec::with_capacity(FFT_SIZE * 2),
            analyzer: SpectrumAnalyzer::new(sample_rate as f32),
        }
    }
}

fn empty_row(label: String) -> SpectrumRow {
    SpectrumRow {
        label: label.into(),
        levels: ModelRc::from(Rc::new(VecModel::from(vec![0.0_f32; N_BANDS]))),
        peaks: ModelRc::from(Rc::new(VecModel::from(vec![0.0_f32; N_BANDS]))),
        active: false,
    }
}

fn snapshot_to_slint_arrays(
    snap: &SpectrumSnapshot,
) -> (ModelRc<f32>, ModelRc<f32>) {
    let levels: Vec<f32> = snap.levels.to_vec();
    let peaks: Vec<f32> = snap.peaks.to_vec();
    (
        ModelRc::from(Rc::new(VecModel::from(levels))),
        ModelRc::from(Rc::new(VecModel::from(peaks))),
    )
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

pub struct SpectrumSession {
    rows_model: Rc<VecModel<SpectrumRow>>,
    row_states: Vec<RowState>,
    fingerprint: String,
}

impl SpectrumSession {
    /// Build a spectrum session for the given project: subscribe taps for
    /// every active output channel of every enabled chain. Stereo output
    /// modes get a single combined-channel row (currently shows L only —
    /// matches the original block's behavior of a single 63-band view per
    /// terminal output entry).
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

                        let chain_label = chain
                            .description
                            .clone()
                            .unwrap_or_else(|| chain.id.0.clone());

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
                            let label = format!(
                                "{}  ·  OUT {}  ·  CH {}{}",
                                chain_label.to_uppercase(),
                                output_index + 1,
                                channel + 1,
                                ch_label
                            );
                            rows_model.push(empty_row(label));
                            row_states.push(RowState::new(ring, sample_rate));
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

    /// Drain rings, run the FFT when enough samples accumulated, update
    /// the row model. Call from a UI timer (~30 Hz).
    pub fn tick(&mut self) {
        for (idx, state) in self.row_states.iter_mut().enumerate() {
            while let Some(s) = state.ring.pop() {
                if state.sample_buf.len() >= FFT_SIZE * 2 {
                    state.sample_buf.drain(..FFT_SIZE);
                }
                state.sample_buf.push(s);
            }
            while state.sample_buf.len() >= FFT_SIZE {
                let buf: Vec<f32> = state.sample_buf.drain(..FFT_SIZE).collect();
                let snap = state.analyzer.process(&buf);
                let (levels, peaks) = snapshot_to_slint_arrays(&snap);
                if let Some(mut row) = self.rows_model.row_data(idx) {
                    row.levels = levels;
                    row.peaks = peaks;
                    row.active = snap.peaks.iter().any(|&p| p > 0.05);
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
        // Both arrays have N_BANDS slots.
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
    fn row_state_starts_with_empty_buffer() {
        let ring = Arc::new(SpscRing::new(FFT_SIZE * 2, 0.0_f32));
        let state = RowState::new(ring, 48_000);
        assert!(state.sample_buf.is_empty());
        assert!(state.sample_buf.capacity() >= FFT_SIZE * 2);
    }
}
