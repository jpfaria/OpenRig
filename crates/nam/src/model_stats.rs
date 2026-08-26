//! Responsibility: counts how many NAM models this process has open.

static MODELS_CREATED: AtomicUsize = AtomicUsize::new(0);
static MODELS_LIVE: AtomicUsize = AtomicUsize::new(0);

use crate::GENERIC_NAM_MODEL_ID;
use std::sync::atomic::{AtomicUsize, Ordering};

/// Total NAM models loaded since process start (monotonic). See
/// [`MODELS_CREATED`].
pub fn models_created() -> usize {
    MODELS_CREATED.load(Ordering::Relaxed)
}

/// NAM models currently held in memory. See [`MODELS_LIVE`].
pub fn live_models() -> usize {
    MODELS_LIVE.load(Ordering::Relaxed)
}

pub fn supports_model(model: &str) -> bool {
    model == GENERIC_NAM_MODEL_ID
}

pub use crate::params::{
    model_schema, plugin_parameter_specs, plugin_parameter_specs_with_defaults,
    slim_parameter_spec, AMP_GROUP, EQ_GROUP, NOISE_GATE_GROUP,
};

/// Bump the counters when a model is instantiated.
pub(crate) fn note_model_created() {
    MODELS_CREATED.fetch_add(1, Ordering::Relaxed);
    MODELS_LIVE.fetch_add(1, Ordering::Relaxed);
}

/// Drop one from the live count when a model is released.
pub(crate) fn note_model_dropped() {
    MODELS_LIVE.fetch_sub(1, Ordering::Relaxed);
}
