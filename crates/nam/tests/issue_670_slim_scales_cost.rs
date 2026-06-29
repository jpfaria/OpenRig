//! Issue #670 — the user set slim=0 on both NAM/A2 blocks and the live cost
//! did NOT drop. If the A2 `slim` knob is inert (e.g. the wrapper's
//! dynamic_cast<SlimmableModel*> comes back null and the call is silently
//! skipped), the rig always pays full-size inference. RED if slim does not
//! scale the inference cost.
#![cfg(not(debug_assertions))]

use block_core::MonoProcessor;
use nam::processor::{NamProcessor, DEFAULT_PLUGIN_PARAMS};
use std::time::Instant;

fn od808() -> String {
    format!(
        "{}/../engine/tests/fixtures/plugins/nam/maxon_od808_a2/captures/od808_2pm_2pm_plus6_a2.nam",
        env!("CARGO_MANIFEST_DIR")
    )
}

fn median_block_ns(slim: f32) -> u128 {
    let mut params = DEFAULT_PLUGIN_PARAMS;
    params.noise_gate_enabled = false;
    params.slim_size = slim;
    let mut proc = NamProcessor::new(&od808(), None, params, 48_000.0).expect("od808 A2 must load");
    let mut buf: Vec<f32> = (0..64)
        .map(|i| 0.3 * (2.0 * std::f32::consts::PI * 220.0 * i as f32 / 48_000.0).sin())
        .collect();
    for _ in 0..512 {
        proc.process_block(&mut buf);
    }
    let mut s = Vec::with_capacity(2000);
    for _ in 0..2000 {
        let t0 = Instant::now();
        proc.process_block(&mut buf);
        s.push(t0.elapsed().as_nanos());
    }
    s.sort_unstable();
    s[s.len() / 2]
}

#[test]
fn slim_zero_is_meaningfully_lighter_than_full() {
    let full = median_block_ns(1.0);
    let slim = median_block_ns(0.0);
    eprintln!(
        "[#670 SLIM] od808 A2 per 64-frame buffer: slim=1.0 -> {}us  slim=0.0 -> {}us  ratio={:.2}x",
        full / 1000,
        slim / 1000,
        slim as f64 / full.max(1) as f64,
    );
    assert!(
        slim * 10 < full * 8,
        "BUG #670: slim=0.0 costs {}us vs {}us at full size (>80%) — the A2 \
         slim knob is NOT reducing inference cost (inert wiring: the wrapper's \
         SlimmableModel cast/staging is not taking effect).",
        slim / 1000,
        full / 1000,
    );
}

/// Same check through the REGISTRY route the engine uses
/// (plugin_loader::registry::find + package.build_processor) — isolates
/// whether the slim loss is in from_package/dispatch.
#[test]
fn slim_zero_via_registry_route() {
    use block_core::param::ParameterSet;
    use block_core::AudioChannelLayout;
    use domain::value_objects::ParameterValue;

    nam::register_builder();
    plugin_loader::registry::init(std::path::Path::new(&format!(
        "{}/../engine/tests/fixtures/plugins",
        env!("CARGO_MANIFEST_DIR")
    )));
    let pkg = plugin_loader::registry::find("nam_maxon_od808_a2").expect("registered");

    let cost = |slim: Option<f32>| -> u128 {
        let mut params = ParameterSet::default();
        params.insert("drive", ParameterValue::Float(12.0));
        params.insert("tone", ParameterValue::Float(12.0));
        params.insert("boost", ParameterValue::String("plus6".into()));
        params.insert("noise_gate.enabled", ParameterValue::Bool(false));
        if let Some(s) = slim {
            params.insert("slim", ParameterValue::Float(s));
        }
        let built = pkg
            .build_processor(&params, 48_000.0, AudioChannelLayout::Mono)
            .expect("build");
        let mut proc = match built {
            block_core::BlockProcessor::Mono(p) => p,
            _ => panic!("mono expected"),
        };
        let mut buf: Vec<f32> = (0..64)
            .map(|i| 0.3 * (2.0 * std::f32::consts::PI * 220.0 * i as f32 / 48_000.0).sin())
            .collect();
        for _ in 0..512 {
            proc.process_block(&mut buf);
        }
        let mut s = Vec::with_capacity(1500);
        for _ in 0..1500 {
            let t0 = std::time::Instant::now();
            proc.process_block(&mut buf);
            s.push(t0.elapsed().as_nanos());
        }
        s.sort_unstable();
        s[s.len() / 2]
    };

    let full = cost(None);
    let slim0 = cost(Some(0.0));
    eprintln!(
        "[#670 SLIMREG] registry route: default -> {}us  slim=0 -> {}us  ratio={:.2}x",
        full / 1000,
        slim0 / 1000,
        slim0 as f64 / full.max(1) as f64,
    );
    assert!(
        slim0 * 10 < full * 8,
        "BUG #670: registry route ignores slim ({}us vs {}us)",
        slim0 / 1000,
        full / 1000,
    );
}
