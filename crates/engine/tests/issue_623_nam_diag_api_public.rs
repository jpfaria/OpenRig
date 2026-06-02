//! Issue #623 req #2: the NAM offline-diagnostics API must stay public
//! and importable. The #612 FFI rewrite dropped `open_model_diag` /
//! `close_model_diag` and made `nam_process` a private extern, which
//! broke the OpenRig-plugins catalog-audit gate (E0603/E0432). This test
//! locks all three as `pub` paths under `nam::processor` and round-trips
//! the bundled A2 model through them so the symbols are not just present
//! but actually usable.

use std::path::PathBuf;

// The load-bearing assertion of this test: these three paths must be
// public and importable. If any regresses to private/removed, this fails
// to COMPILE — that is the lock.
use nam::processor::{close_model_diag, nam_process, open_model_diag};

fn a2_capture() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("tests/fixtures/plugins/nam/a2_slimmable/captures/r7hqwhd3s2_a2.nam")
}

#[test]
fn nam_diag_api_is_public_and_round_trips() {
    let path = a2_capture();
    let handle = open_model_diag(path.to_str().expect("utf8 path"))
        .expect("open_model_diag must load the A2 model");

    let input: Vec<f32> = (0..2_048)
        .map(|n| 0.3 * (2.0 * std::f32::consts::PI * 110.0 * n as f32 / 48_000.0).sin())
        .collect();
    let mut output = vec![0.0f32; input.len()];
    unsafe { nam_process(handle, &input, &mut output) };

    assert!(
        output.iter().all(|s| s.is_finite()),
        "diag nam_process produced NaN/Inf"
    );

    unsafe { close_model_diag(handle) };
}
