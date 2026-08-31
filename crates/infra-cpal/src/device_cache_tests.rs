//! #913 — when the device snapshot still counts as fresh.
//!
//! Freshness is the whole decision this module makes: fresh serves the cached
//! list, stale serves the previous one and refreshes off-thread (#693), and no
//! snapshot at all is the only case that enumerates inline. Getting "never
//! fetched" wrong would put a CoreAudio enumeration (~2s measured) on the UI
//! thread every refresh tick.

use super::{TimedDeviceCache, DEVICE_CACHE_TTL};
use std::time::Instant;

#[test]
fn a_cache_that_never_fetched_is_not_fresh() {
    assert!(!TimedDeviceCache::new().is_fresh());
}

#[test]
fn a_cache_fetched_just_now_is_fresh() {
    let cache = TimedDeviceCache {
        devices: Some(Vec::new()),
        fetched_at: Some(Instant::now()),
    };
    assert!(cache.is_fresh());
}

#[test]
fn a_cache_older_than_the_ttl_is_stale() {
    let cache = TimedDeviceCache {
        devices: Some(Vec::new()),
        fetched_at: Instant::now().checked_sub(DEVICE_CACHE_TTL),
    };
    assert!(
        !cache.is_fresh(),
        "exactly at the TTL the snapshot has expired"
    );
}

#[test]
fn an_empty_device_list_is_still_a_snapshot() {
    // An interface-less machine legitimately enumerates to nothing. Treating
    // that as "never fetched" would re-enumerate on every tick.
    let cache = TimedDeviceCache {
        devices: Some(Vec::new()),
        fetched_at: Some(Instant::now()),
    };
    assert!(cache.is_fresh());
    assert!(cache.devices.is_some());
}
