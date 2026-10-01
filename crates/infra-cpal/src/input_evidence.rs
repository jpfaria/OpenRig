//! Responsibility: describes the evidence kept of an input stream when its chain restarts.
//!
//! The automatic restart of a chain whose input arrives stepped cuts the sound
//! and does not explain the fault. Each restart therefore takes this evidence
//! first: the last seconds of every input stream of the chain as the HAL
//! delivered them, the timing of each callback, and the chain's output routes.

use std::time::SystemTime;

use engine::runtime_output_route_stats::OutputRouteStats;
use serde::Serialize;

/// Which stream the evidence comes from, as it was opened.
#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct InputStreamIdentity {
    pub chain_id: String,
    pub input_index: usize,
    /// cpal device id (`coreaudio:<uid>` on macOS).
    pub device_id: Option<String>,
    /// Rate the stream was opened at.
    pub sample_rate: u32,
    /// Buffer size the stream asked for.
    pub buffer_frames: u32,
    /// Interleaved channels of each callback (every channel of the device).
    pub channels: usize,
    pub opened_at: SystemTime,
}

/// One input callback: when the HAL called it and which frames it carried.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
pub struct CycleRecord {
    /// Host time of the callback, in ns since boot.
    pub host_ns: u64,
    /// Stream frame index of the callback's first frame.
    pub first_frame: u64,
    pub frames: u32,
}

/// The last seconds of one input stream.
#[derive(Debug, Clone, PartialEq, Default)]
pub struct InputEvidenceSnapshot {
    /// Stream frame index of the first frame in `samples`.
    pub first_frame: u64,
    /// Interleaved, every channel of the device.
    pub samples: Vec<f32>,
    pub cycles: Vec<CycleRecord>,
}

/// One input stream of the chain at the restart.
#[derive(Debug, Clone, PartialEq)]
pub struct StreamEvidence {
    pub identity: InputStreamIdentity,
    pub snapshot: InputEvidenceSnapshot,
}

/// Everything OpenRig knew about a chain when its input tripped as stepped.
#[derive(Debug, Clone)]
pub struct SteppedInputEvidence {
    pub chain_id: String,
    pub taken_at: SystemTime,
    /// Whether the chain's runtime was still marked stepped.
    pub input_stepped: bool,
    pub streams: Vec<StreamEvidence>,
    /// Output routes per runtime group, as `openrig://routes` shows them.
    pub routes: Vec<(usize, Vec<OutputRouteStats>)>,
}
