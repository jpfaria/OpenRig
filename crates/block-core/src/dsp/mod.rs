//! DSP primitives shared by every `block-*` crate. Each submodule owns
//! one self-contained building block. This file is re-exports only.

pub mod denormal;
pub mod hilbert_iir;
pub mod legacy;
pub mod lfo;
pub mod oversampling;
pub mod svf;

// Legacy primitives lifted from the previous flat `dsp.rs`. New code
// should import from the dedicated submodules above when an equivalent
// exists; legacy types are kept stable so the existing block-* crates
// keep building unchanged.
pub use legacy::{
    calculate_coefficient, capitalize_first, db_to_lin, lin_to_db, BiquadFilter, BiquadKind,
    EnvelopeFollower, OnePoleHighPass, OnePoleLowPass, BIQUAD_COEFF_RAMP_FRAMES,
};

// Friendly aliases so new plugin code reads with intent rather than
// implementation. `OnePoleHighPass` at ~5 Hz IS a DC blocker — alias it.
pub use legacy::OnePoleHighPass as DcBlocker;

pub use denormal::{flush_denormal, DENORMAL_GUARD};
pub use hilbert_iir::HilbertIir;
pub use lfo::{Lfo, LfoShape};
pub use oversampling::Oversampler2x;
pub use svf::{Svf, SvfFrame};
