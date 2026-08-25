//! Responsibility: keeps the historical `runtime_state_taps` path alive for importers.
//!
//! The methods moved to `runtime_taps_subscribe.rs`, `runtime_taps_lifecycle.rs`
//! and `runtime_stream_query.rs` (#873). Inherent methods need no import, so
//! nothing else had to change.
