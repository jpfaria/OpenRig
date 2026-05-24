//! `meter_wiring.rs` accumulated backwards-compat helpers (legacy
//! single-pair API + an eager "always re-subscribe" variant) during
//! the iterative per-stream / lazy / flicker fixes. Production no
//! longer calls any of them, but every one ships as a `pub fn` /
//! `pub struct`, so each one triggers a `dead_code` warning. User
//! report (May 24): "esta cheio de warning..".
//!
//! Pin the cleanup: these public items must be gone from the source.

use std::path::PathBuf;

fn src() -> String {
    let path = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("src/meter_wiring.rs");
    std::fs::read_to_string(&path).unwrap_or_else(|e| panic!("read {}: {e}", path.display()))
}

#[test]
fn meter_wiring_has_no_unused_legacy_api() {
    let s = src();
    for needle in [
        "pub struct ChainMeterRings",
        "fn first_stream_or_default",
        "pub type MeterStore =",
        "pub fn new_meter_store(",
        "pub fn refresh_subscriptions_per_stream<",
        "pub fn stream_meter_rows_from_readings(",
        "pub fn ensure_subscribed(",
        "pub fn refresh_subscriptions<",
        "pub fn prune_dead(",
        "pub fn poll_all(",
    ] {
        assert!(
            !s.contains(needle),
            "meter_wiring.rs still exposes dead legacy API: `{needle}`"
        );
    }
}
