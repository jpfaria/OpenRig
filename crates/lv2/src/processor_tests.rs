//! #328: what an LV2 latency port value becomes as a sample count.

use super::latency_from_port;

#[test]
fn a_reported_latency_becomes_whole_samples() {
    assert_eq!(latency_from_port(64.0), 64);
    assert_eq!(latency_from_port(63.6), 64);
    assert_eq!(latency_from_port(-3.0), 0, "a negative report means none");
    assert_eq!(latency_from_port(f32::NAN), 0, "a broken report means none");
    assert_eq!(latency_from_port(f32::INFINITY), 0);
}
