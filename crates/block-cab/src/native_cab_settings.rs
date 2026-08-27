//! Responsibility: describes how a native cab is configured.

#[derive(Debug, Clone, Copy)]
pub struct NativeCabSettings {
    pub low_cut_hz: f32,
    pub high_cut_hz: f32,
    pub resonance: f32,
    pub air: f32,
    pub mic_position: f32,
    pub mic_distance: f32,
    pub room_mix: f32,
    pub output: f32,
}

/// Per-model magnitude-response fingerprint, approximating the measured response
/// of a reference cabinet with a biquad cascade. These are *descriptive targets*
/// (a 4x12 with Celestion-style speakers, a small warm 1x12, a bright scooped
/// 2x12) — never a named/branded model (zero-coupling rule). The biquad cascade
/// matches the magnitude curve; it does not reproduce the comb-filtering or
/// complex phase of a real cabinet — that only comes from a measured IR.
#[derive(Debug, Clone, Copy)]
pub struct NativeCabProfile {
    /// Speaker high-frequency rolloff corner — the dominant cabinet trait.
    /// Applied as two cascaded low-passes (~24 dB/oct), the steep skirt a real
    /// cone has above its top end.
    pub rolloff_hz: f32,
    pub rolloff_q: f32,
    /// Low-end cone/cabinet resonance bump.
    pub low_bump_hz: f32,
    pub low_bump_db: f32,
    pub low_bump_q: f32,
    /// Mid scoop — the guitar-cab "honk" notch; its centre and depth strongly
    /// separate one cabinet from another.
    pub mid_dip_hz: f32,
    pub mid_dip_db: f32,
    pub mid_dip_q: f32,
    /// Presence/bite peak in the upper mids.
    pub presence_hz: f32,
    pub presence_db: f32,
    pub presence_q: f32,
    /// Room reflection tap (kept from the previous engine).
    pub room_base_ms: f32,
    pub room_span_ms: f32,
}

#[derive(Debug, Clone, Copy)]
pub struct NativeCabSchemaDefaults {
    pub low_cut_hz: f32,
    pub high_cut_hz: f32,
    pub resonance: f32,
    pub air: f32,
    pub mic_position: f32,
    pub mic_distance: f32,
    pub room_mix: f32,
}
