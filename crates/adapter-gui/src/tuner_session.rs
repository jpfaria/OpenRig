//! TunerWindow live session — owns the per-row sample taps, accumulators and
//! pitch detectors that drive the row model.
//!
//! The audio thread pushes samples into per-channel SPSC rings (registered via
//! [`infra_cpal::ProjectRuntimeController::subscribe_input_tap`]). On a UI
//! timer the session drains each ring into a small accumulator buffer; once
//! the buffer reaches `pitch_yin::BUFFER_SIZE` (≈85 ms @ 48 kHz) it is fed
//! to a [`PitchDetector`] and the resulting `PitchUpdate` is reflected on the
//! row model via [`TunerRow`].

use std::sync::Arc;

use engine::spsc::SpscRing;
use feature_dsp::pitch_yin::{PitchDetector, PitchUpdate, BUFFER_SIZE};
use infra_cpal::ProjectRuntimeController;
use project::block::AudioBlockKind;
use project::project::Project;
use slint::{Model, ModelRc, SharedString, VecModel};
use std::rc::Rc;

use crate::TunerRow;

/// Tuner default reference (440 Hz). Per-row reference would require a UI control.
const REFERENCE_HZ: f32 = 440.0;
/// Capacity per channel ring: ≥ BUFFER_SIZE × 2 so we never lose samples between
/// UI ticks under any reasonable timer cadence.
const RING_CAPACITY: usize = BUFFER_SIZE * 4;

/// One pitch-detection pipeline per (chain, input, channel).
struct RowState {
    ring: Arc<SpscRing<f32>>,
    sample_buf: Vec<f32>,
    detector: PitchDetector,
}

impl RowState {
    fn new(ring: Arc<SpscRing<f32>>, sample_rate: usize, reference_hz: f32) -> Self {
        Self {
            ring,
            sample_buf: Vec::with_capacity(BUFFER_SIZE * 2),
            detector: PitchDetector::new(sample_rate, reference_hz),
        }
    }
}

fn placeholder_row(label: String) -> TunerRow {
    TunerRow {
        label: label.into(),
        note: SharedString::from("—"),
        octave: 0,
        cents: 0.0,
        frequency: 0.0,
        active: false,
    }
}

/// Compute the octave number (scientific pitch notation, A4 = octave 4) from
/// a frequency. Uses the configured reference Hz so 432 Hz tuning still maps
/// "A" to octave 4 around 432.
fn freq_to_octave(freq: f32, reference_hz: f32) -> i32 {
    if freq <= 0.0 {
        return 0;
    }
    let semitones_from_a4 = 12.0 * (freq / reference_hz).log2();
    let midi = (69.0 + semitones_from_a4.round()) as i32;
    midi / 12 - 1
}

/// Stable signature of every (chain, input, channel) the tuner cares about.
/// Compared at every tick to detect when the user enables/disables chains
/// or edits an InputBlock so we can tear down and rebuild the session
/// without forcing the user to close and reopen the window.
fn project_input_fingerprint(project: &Project) -> String {
    let mut s = String::new();
    for chain in &project.chains {
        if !chain.enabled {
            continue;
        }
        s.push_str(&chain.id.0);
        s.push('@');
        let mut input_index = 0_usize;
        for block in &chain.blocks {
            if let AudioBlockKind::Input(input) = &block.kind {
                for entry in &input.entries {
                    s.push_str(&format!("[{}/{}/", input_index, entry.device_id.0));
                    for ch in &entry.channels {
                        s.push_str(&format!("{},", ch));
                    }
                    s.push_str(&format!("/{:?}]", entry.mode));
                    input_index += 1;
                }
            }
        }
        s.push(';');
    }
    s
}

pub struct TunerSession {
    rows_model: Rc<VecModel<TunerRow>>,
    row_states: Vec<RowState>,
    fingerprint: String,
}

impl TunerSession {
    /// Build a tuner session for the given project: subscribe taps for every
    /// active input channel of every enabled chain.
    pub fn build(project: &Project, controller: &ProjectRuntimeController) -> Self {
        let rows_model: Rc<VecModel<TunerRow>> = Rc::new(VecModel::from(Vec::<TunerRow>::new()));
        let mut row_states: Vec<RowState> = Vec::new();

        for chain in &project.chains {
            if !chain.enabled {
                continue;
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
                            .map(|d| d.sample_rate)
                    }),
                    _ => None,
                })
                .unwrap_or(48_000) as usize;

            let mut input_index = 0_usize;
            for block in &chain.blocks {
                if let AudioBlockKind::Input(input) = &block.kind {
                    for entry in &input.entries {
                        if entry.channels.is_empty() {
                            input_index += 1;
                            continue;
                        }
                        let max_channel = *entry.channels.iter().max().unwrap_or(&0);
                        let total_channels = max_channel + 1;

                        let rings = controller.subscribe_input_tap(
                            &chain.id,
                            input_index,
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
                                "{}  ·  IN {}  ·  CH {}{}",
                                chain_label.to_uppercase(),
                                input_index + 1,
                                channel + 1,
                                ch_label
                            );
                            rows_model.push(placeholder_row(label));
                            row_states.push(RowState::new(ring, sample_rate, REFERENCE_HZ));
                        }

                        input_index += 1;
                    }
                }
            }
        }

        Self {
            rows_model,
            row_states,
            fingerprint: project_input_fingerprint(project),
        }
    }

    pub fn rows_model_rc(&self) -> ModelRc<TunerRow> {
        ModelRc::from(self.rows_model.clone())
    }

    /// Cheap re-fingerprint check. Returns `true` when the input topology
    /// changed (chain enable/disable, input edit, channel change) since
    /// this session was built — the caller should rebuild.
    pub fn needs_rebuild(&self, project: &Project) -> bool {
        self.fingerprint != project_input_fingerprint(project)
    }

    /// Drain rings, run the detector when enough samples accumulated, and
    /// update the row model. Call from a UI timer (~30 Hz is plenty).
    pub fn tick(&mut self) {
        for (idx, state) in self.row_states.iter_mut().enumerate() {
            while let Some(s) = state.ring.pop() {
                if state.sample_buf.len() >= BUFFER_SIZE * 2 {
                    state.sample_buf.drain(..BUFFER_SIZE);
                }
                state.sample_buf.push(s);
            }
            while state.sample_buf.len() >= BUFFER_SIZE {
                let mut buf: Vec<f32> = state.sample_buf.drain(..BUFFER_SIZE).collect();
                // Auto-gain: the tap reads instrument-level signal pre-FX,
                // typically -20 dBFS or quieter. Normalize the buffer to a
                // target peak (~0.3) so YIN sees a strong, consistent
                // amplitude regardless of the user's pickup output level
                // or device input gain. Cap the gain at 30 dB so a noisy
                // silence does not get amplified into spurious detections.
                let peak = buf
                    .iter()
                    .fold(0.0_f32, |a, &s| a.max(s.abs()));
                if peak > 0.001 {
                    const TARGET_PEAK: f32 = 0.3;
                    const MAX_GAIN: f32 = 32.0; // ≈ 30 dB
                    let gain = (TARGET_PEAK / peak).min(MAX_GAIN);
                    for s in buf.iter_mut() {
                        *s *= gain;
                    }
                }
                match state.detector.process_buffer(&buf) {
                    PitchUpdate::Update { note, cents, freq } => {
                        if let Some(mut row) = self.rows_model.row_data(idx) {
                            row.note = note.into();
                            row.octave = freq_to_octave(freq, REFERENCE_HZ);
                            row.cents = cents;
                            row.frequency = freq;
                            row.active = true;
                            self.rows_model.set_row_data(idx, row);
                        }
                    }
                    PitchUpdate::Silence => {
                        if let Some(mut row) = self.rows_model.row_data(idx) {
                            row.note = SharedString::from("—");
                            row.octave = 0;
                            row.cents = 0.0;
                            row.frequency = 0.0;
                            row.active = false;
                            self.rows_model.set_row_data(idx, row);
                        }
                    }
                    PitchUpdate::NoChange => {}
                }
            }
        }
    }
}

#[cfg(test)]
#[path = "tuner_session_tests.rs"]
mod tests;
