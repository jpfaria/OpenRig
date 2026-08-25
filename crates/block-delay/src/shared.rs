//! Responsibility: keeps the historical `shared` path pointing at the delay primitives.

pub use crate::channel_adapters::{build_dual_mono_from_builder, DualMonoProcessor, StereoToMono};
pub use crate::delay_line::DelayLine;
pub use crate::delay_math::{
    clamp_feedback, clamp_mix, clamp_time_ms, lowpass_step, mix_dry_wet, process_simple_delay,
    sanitize, soft_saturate,
};
pub use crate::delay_math::{MAX_DELAY_MS, MAX_FEEDBACK, MIN_DELAY_MS};
