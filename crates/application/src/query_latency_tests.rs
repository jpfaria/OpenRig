use super::*;
use crate::local_dispatcher_tests::{make_core_block, make_project};
use serde_json::Value;

#[test]
fn latency_report_names_the_chain_and_the_rate_it_probed_at() {
    let project = make_project("chain_0", make_core_block("blk_0", true));

    let json: Value = serde_json::from_str(
        &chain_latency_report(&project.borrow(), &[], &ChainId("chain_0".into()), 44_100.0)
            .expect("known chain reports"),
    )
    .unwrap();

    assert_eq!(json["chain"], "chain_0");
    // No saved device setting for this chain → the live rate is used,
    // never a hardcoded 48 kHz (issue #723).
    assert_eq!(json["sample_rate"], 44_100.0);
    assert_eq!(json["buffer_frames"], DEFAULT_PROBE_BUFFER_FRAMES);
    assert!(json["dsp_latency_ms"].as_f64().unwrap() >= 0.0);
}

#[test]
fn latency_report_rejects_an_unknown_chain() {
    let project = make_project("chain_0", make_core_block("blk_0", true));

    let result = chain_latency_report(
        &project.borrow(),
        &[],
        &ChainId("no-such-chain".into()),
        48_000.0,
    );

    assert!(result.is_err(), "unknown chain must not report a latency");
}
