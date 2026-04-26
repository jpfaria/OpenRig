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

/// Initial empty TunerRow — `active=false`, "—" placeholders.
fn placeholder_row(label: String) -> TunerRow {
    TunerRow {
        label: label.into(),
        note: SharedString::from("—"),
        cents: 0.0,
        frequency: 0.0,
        active: false,
    }
}

pub struct TunerSession {
    rows_model: Rc<VecModel<TunerRow>>,
    row_states: Vec<RowState>,
}

impl TunerSession {
    /// Build a tuner session for the given project: subscribe taps for every
    /// active input channel of every enabled chain.
    ///
    /// Returns `None` if no chains are running (no runtimes registered yet).
    pub fn build(
        project: &Project,
        controller: &ProjectRuntimeController,
    ) -> Self {
        let rows: Vec<TunerRow> = Vec::new();
        let rows_model: Rc<VecModel<TunerRow>> = Rc::new(VecModel::from(rows));
        let mut row_states: Vec<RowState> = Vec::new();

        // Tuner default reference (440 Hz). Could later read from a global
        // setting; per-row reference would require a UI control.
        const REFERENCE_HZ: f32 = 440.0;
        // Capacity per channel ring: ≥ BUFFER_SIZE × 2 so we never lose
        // samples between UI ticks under any reasonable timer cadence.
        const RING_CAPACITY: usize = BUFFER_SIZE * 4;

        for chain in &project.chains {
            if !chain.enabled {
                continue;
            }

            // Sample rate per chain: most projects share one rate. Default
            // to 48 kHz if device_settings has no entry.
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

            // Track input_index across InputBlock entries (Insert returns
            // come after, but we don't tap them — only real Inputs).
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

                        // Subscribe one ring per channel.
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

                        for (ch_pos, (channel, ring)) in entry
                            .channels
                            .iter()
                            .zip(rings.into_iter())
                            .enumerate()
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
                                "{}  ·  in {}  ·  ch {}{}",
                                chain_label,
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
        }
    }

    pub fn rows_model_rc(&self) -> ModelRc<TunerRow> {
        ModelRc::from(self.rows_model.clone())
    }

    /// Drain rings, run the detector when enough samples accumulated, and
    /// update the row model. Call from a UI timer (~30 Hz is plenty).
    pub fn tick(&mut self) {
        for (idx, state) in self.row_states.iter_mut().enumerate() {
            // Drain the entire ring into the accumulator. Cap the buffer
            // at 2× BUFFER_SIZE so a slow consumer or a paused timer
            // doesn't grow it unbounded.
            while let Some(s) = state.ring.pop() {
                if state.sample_buf.len() >= BUFFER_SIZE * 2 {
                    state.sample_buf.drain(..BUFFER_SIZE);
                }
                state.sample_buf.push(s);
            }
            // Run detection in fixed-size chunks while we have enough.
            while state.sample_buf.len() >= BUFFER_SIZE {
                let buf: Vec<f32> = state.sample_buf.drain(..BUFFER_SIZE).collect();
                match state.detector.process_buffer(&buf) {
                    PitchUpdate::Update {
                        note,
                        cents,
                        freq,
                    } => {
                        if let Some(mut row) = self.rows_model.row_data(idx) {
                            row.note = note.into();
                            row.cents = cents;
                            row.frequency = freq;
                            row.active = true;
                            self.rows_model.set_row_data(idx, row);
                        }
                    }
                    PitchUpdate::Silence => {
                        if let Some(mut row) = self.rows_model.row_data(idx) {
                            row.note = SharedString::from("—");
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
