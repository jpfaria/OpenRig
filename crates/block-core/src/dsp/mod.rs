//! Responsibility: routes the DSP primitives the block crates share.

pub mod biquad;
pub mod denormal;
pub mod envelope;
pub mod gain;
pub mod hilbert_iir;
pub mod lfo;
pub mod one_pole;
pub mod oversampling;
pub mod svf;

pub use biquad::{BiquadDesign, BiquadFilter, BiquadKind, BIQUAD_COEFF_RAMP_FRAMES};
pub use envelope::{calculate_coefficient, EnvelopeFollower};
pub use gain::{db_to_lin, lin_to_db};
pub use one_pole::{OnePoleHighPass, OnePoleLowPass};

// The importers predate the split and still reach for `dsp::capitalize_first`.
pub use crate::text::capitalize_first;

// Friendly aliases so new plugin code reads with intent rather than
// implementation. `OnePoleHighPass` at ~5 Hz IS a DC blocker — alias it.
pub use one_pole::OnePoleHighPass as DcBlocker;

pub use denormal::{flush_denormal, DENORMAL_GUARD};
pub use hilbert_iir::HilbertIir;
pub use lfo::{Lfo, LfoShape};
pub use oversampling::Oversampler2x;
pub use svf::{Svf, SvfFrame};
