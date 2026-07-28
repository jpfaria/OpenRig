//! #829: analyzer readings as transport-neutral JSON.
//!
//! The tuner and the spectrum are observation windows the GUI has always
//! had for itself: `SetTunerEnabled` / `SetSpectrumEnabled` power them on,
//! but the numbers they produce lived only inside `adapter-gui`. Per the
//! read-parity law every window the user looks at is also a `QueryKind`,
//! so MCP / gRPC read the same values from the same serializer.
//!
//! The frontend collects the readings (it owns the analyzer sessions) and
//! hands them here; this module owns the wire shape and nothing else.

use serde::Serialize;

/// One tuner row: a single (chain, input, channel) pitch pipeline.
#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct TunerReading {
    /// Chain the tap belongs to (`chain.id.0`).
    pub chain: String,
    /// Per-chain input index — the engine's runtime input index.
    pub input: usize,
    /// Device channel number this row listens to.
    pub channel: usize,
    /// Display label the GUI shows for the same row.
    pub label: String,
    /// Detected note name, or `"—"` while nothing is detected.
    pub note: String,
    /// Scientific-pitch octave of `note` (0 when idle).
    pub octave: i32,
    /// Deviation from the tempered note, in cents.
    pub cents: f32,
    /// Detected fundamental in Hz (0 when idle).
    pub frequency: f32,
    /// `true` once the row has a confident detection.
    pub active: bool,
}

/// One spectrum row: the band levels of a single (chain, input, channel) tap.
#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct SpectrumReading {
    pub chain: String,
    pub input: usize,
    pub channel: usize,
    pub label: String,
    /// Per-band level, 0.0..1.0, one entry per band in `band_hz`.
    pub levels: Vec<f32>,
    /// Per-band peak hold, same length as `levels`.
    pub peaks: Vec<f32>,
    pub active: bool,
}

#[derive(Serialize)]
struct TunerPayload<'a> {
    running: bool,
    reference_hz: f32,
    rows: &'a [TunerReading],
}

#[derive(Serialize)]
struct SpectrumPayload<'a> {
    running: bool,
    band_hz: &'a [f32],
    rows: &'a [SpectrumReading],
}

/// Serialize the tuner window's live readings.
///
/// `running` is `false` when the tuner is powered off — the rows are then
/// empty, and a client knows to dispatch `SetTunerEnabled` first instead of
/// reading a stale silence as "no signal".
pub fn tuner_readings_json(running: bool, reference_hz: f32, rows: &[TunerReading]) -> String {
    serde_json::to_string(&TunerPayload {
        running,
        reference_hz,
        rows,
    })
    .unwrap_or_else(|e| format!("{{\"error\":\"{e}\"}}"))
}

/// Serialize the spectrum window's live readings. `band_hz` carries the
/// band center frequencies once for the whole payload (they are identical
/// for every row) so a client can label the bars without re-deriving them.
pub fn spectrum_readings_json(running: bool, band_hz: &[f32], rows: &[SpectrumReading]) -> String {
    serde_json::to_string(&SpectrumPayload {
        running,
        band_hz,
        rows,
    })
    .unwrap_or_else(|e| format!("{{\"error\":\"{e}\"}}"))
}

#[cfg(test)]
#[path = "query_analyzers_tests.rs"]
mod tests;
