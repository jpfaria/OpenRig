//! The #496 noise gate must NOT chop a real note's sustain. Issue #496.
//!
//! A gate that kills the decay hiss is worthless if it also swallows
//! the musical tail. This drives the worst hot calibration (+18.68 dB)
//! with a realistically decaying plucked note and asserts the wet
//! output still rings out: while the note is clearly audible the gate
//! stays open (no premature collapse), and the energy tracks a smooth
//! musical decay rather than a hard cut.

use std::path::PathBuf;

use block_core::param::ParameterSet;
use block_core::{AudioChannelLayout, BlockProcessor};
use domain::value_objects::ParameterValue;
use plugin_loader::discover;
use plugin_loader::discover::LoadedPackage;

const SR: f32 = 48_000.0;
const HOT_CAL_DB: f32 = 18.68;

fn load(rel_dir: &str) -> LoadedPackage {
    let root = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("tests/fixtures/plugins");
    let target = root.join(rel_dir);
    discover::discover(&root)
        .expect("discover")
        .into_iter()
        .filter_map(Result::ok)
        .find(|p| p.root == target)
        .expect("fixture")
}

fn db(x: f32) -> f32 {
    20.0 * x.max(1e-9).log10()
}

fn rms_db(s: &[f32]) -> f32 {
    db((s.iter().map(|v| v * v).sum::<f32>() / s.len() as f32).sqrt())
}

#[test]
fn nam_gate_does_not_chop_a_decaying_note() {
    nam::register_builder();

    let mut pkg = load("nam/marshall_plexi");
    pkg.manifest.output_gain_db = Some(HOT_CAL_DB);
    let mut params = ParameterSet::default();
    params.insert("preset", ParameterValue::String("angus".into()));
    let mut amp = pkg
        .build_processor(&params, SR, AudioChannelLayout::Mono)
        .expect("build");

    // A plucked note: 150 Hz, exponential decay τ = 0.6 s, 2.0 s long.
    // At 1.0 s the dry note is still ≈ -14 dBFS (clearly audible).
    let n = (SR * 2.0) as usize;
    let mut buf: Vec<f32> = (0..n)
        .map(|i| {
            let t = i as f32 / SR;
            0.4 * (-t / 0.6).exp() * (2.0 * std::f32::consts::PI * 150.0 * t).sin()
        })
        .collect();

    match &mut amp {
        BlockProcessor::Mono(m) => m.process_block(&mut buf),
        BlockProcessor::Stereo(_) => panic!("mono"),
    }
    assert!(buf.iter().all(|s| s.is_finite()), "NaN/Inf");

    let win = (SR * 0.05) as usize;
    let at = |sec: f32| {
        let start = (SR * sec) as usize;
        rms_db(&buf[start..start + win])
    };

    // Attack/early sustain must be loud (gate fully open).
    assert!(
        at(0.05) >= -12.0,
        "attack too quiet ({:.1} dBFS) — gate cut the note onset",
        at(0.05)
    );

    // The note is still musically present through the decay: the gate
    // must not have collapsed it while the dry note is well above the
    // noise (dry @1.0s ≈ -14 dBFS, @1.5s ≈ -19 dBFS).
    assert!(
        at(1.0) >= -24.0,
        "sustain chopped at 1.0 s ({:.1} dBFS) — gate too aggressive",
        at(1.0)
    );
    assert!(
        at(1.5) >= -30.0,
        "sustain chopped at 1.5 s ({:.1} dBFS) — gate too aggressive",
        at(1.5)
    );

    // Monotonic-ish decay (no hard cut): later windows quieter than
    // earlier, but never an abrupt >25 dB drop between adjacent 50 ms
    // frames while still musical.
    let mut prev = at(0.05);
    let mut t = 0.1;
    while t <= 1.5 {
        let cur = at(t);
        assert!(
            prev - cur < 25.0,
            "abrupt {:.1} dB drop at {t:.2}s (prev {prev:.1} → {cur:.1}) — \
             gate is chopping, not releasing smoothly",
            prev - cur
        );
        prev = cur;
        t += 0.05;
    }
}
