//! Responsibility: keeps the historical `meter_wiring` path pointing at the four things it held.
//!
//! It was responsible for the dBFS maths, the per-stream tap store, the
//! invalidation detection and writing the rows on screen (#873).

pub use crate::meter_invalidation::{detect_invalidations, project_stream_count};
pub use crate::meter_math::apply_chain_volume_db;
pub(crate) use crate::meter_math::chain_overloaded;
pub use crate::meter_rows::rebuild_stream_meters_row;
pub(crate) use crate::meter_taps::METER_POLL_TICK_MS;
pub use crate::meter_taps::{
    build_streams_from_taps, new_meter_store_per_stream, poll_per_stream,
    refresh_subscriptions_lazy_per_stream, ChainMeterStreams, StreamMeterReading,
};
pub use crate::meter_wiring_poll::start_meter_polling;

// The test modules hang off this path and reach these through `super::`,
// exactly where they were defined before the split (#873).
#[cfg(test)]
pub use crate::meter_invalidation::{chain_meter_signature, timer_chain_signature};
#[cfg(test)]
pub use crate::meter_math::compute_meter_for_chain;
#[cfg(test)]
pub use crate::meter_taps::StreamMeterTaps;

#[cfg(test)]
#[path = "meter_wiring_tests.rs"]
mod tests;

#[cfg(test)]
#[path = "meter_wiring_signature_tests.rs"]
mod signature_tests;
