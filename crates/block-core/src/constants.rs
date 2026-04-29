//! Crate-wide string constants — instrument types, brand sentinels, effect
//! type tags, and instrument list groupings.
//!
//! Lifted out of `lib.rs` (Phase 6 of issue #194). All `pub const` only;
//! no logic. Constants live here so a `String` literal that should be a
//! constant gets caught by `grep '"electric_guitar"' --include='*.rs'`
//! pointing back to a single source of truth.

// Instrument type constants
pub const INST_ELECTRIC_GUITAR: &str = "electric_guitar";
pub const INST_ACOUSTIC_GUITAR: &str = "acoustic_guitar";
pub const INST_BASS: &str = "bass";
pub const INST_VOICE: &str = "voice";
pub const INST_KEYS: &str = "keys";
pub const INST_DRUMS: &str = "drums";
pub const INST_GENERIC: &str = "generic";

// Brand constants
pub const BRAND_NATIVE: &str = "native";

// Effect type constants
pub const EFFECT_TYPE_PREAMP: &str = "preamp";
pub const EFFECT_TYPE_AMP: &str = "amp";
pub const EFFECT_TYPE_FULL_RIG: &str = "full_rig";
pub const EFFECT_TYPE_CAB: &str = "cab";
pub const EFFECT_TYPE_IR: &str = "ir";
pub const EFFECT_TYPE_GAIN: &str = "gain";
pub const EFFECT_TYPE_NAM: &str = "nam";
pub const EFFECT_TYPE_DELAY: &str = "delay";
pub const EFFECT_TYPE_REVERB: &str = "reverb";
pub const EFFECT_TYPE_UTILITY: &str = "utility";
pub const EFFECT_TYPE_DYNAMICS: &str = "dynamics";
pub const EFFECT_TYPE_FILTER: &str = "filter";
pub const EFFECT_TYPE_WAH: &str = "wah";
pub const EFFECT_TYPE_PITCH: &str = "pitch";
pub const EFFECT_TYPE_MODULATION: &str = "modulation";
pub const EFFECT_TYPE_BODY: &str = "body";
pub const EFFECT_TYPE_VST3: &str = "vst3";

// Default instrument (used as fallback)
pub const DEFAULT_INSTRUMENT: &str = INST_ELECTRIC_GUITAR;

/// All non-generic instruments
pub const ALL_INSTRUMENTS: &[&str] = &[
    INST_ELECTRIC_GUITAR,
    INST_ACOUSTIC_GUITAR,
    INST_BASS,
    INST_VOICE,
    INST_KEYS,
    INST_DRUMS,
];

/// Guitar and bass only (for amps, cabs, gain, wah, etc.)
pub const GUITAR_BASS: &[&str] = &[INST_ELECTRIC_GUITAR, INST_BASS];

/// Guitar, acoustic guitar and bass (for preamps)
pub const GUITAR_ACOUSTIC_BASS: &[&str] = &[INST_ELECTRIC_GUITAR, INST_ACOUSTIC_GUITAR, INST_BASS];
