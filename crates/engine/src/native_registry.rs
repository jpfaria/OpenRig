//! Boot-time hook that registers every `block-*` crate's native models
//! into the unified [`plugin_loader`] registry.
//!
//! Centralizing it here keeps adapter-gui from depending on every
//! `block-*` crate directly — engine already does, and the call is
//! cheap (a handful of `Mutex` insertions during startup).
//!
//! Issue: #287

/// Call once at process startup, before `plugin_loader::registry::init`
/// freezes the catalog. Idempotent in practice — each `block-*` crate's
/// `register_natives()` overwrites its own entries by `runtime_id`.
pub fn register_all_natives() {
    block_amp::register_natives();
    block_cab::register_natives();
    block_delay::register_natives();
    block_dyn::register_natives();
    block_filter::register_natives();
    block_gain::register_natives();
    block_mod::register_natives();
    block_preamp::register_natives();
    block_reverb::register_natives();
    block_wah::register_natives();
}
