//! Issue #938 — a stereo effect fed by a mono guitar must leave the two
//! channels different. Before the fix every 2-in/2-out LV2 plugin ran as
//! `DualMono` (one instance per channel, each averaging its own two outputs),
//! so a reverb printed L == R; `ping_pong` fed each input into its own line,
//! so a centred source stayed centred on every echo.
//!
//! Renders a mono (L == R) source through one-block chains with the offline
//! driver — the same processors the live callback runs — and asserts the
//! L×R correlation drops below 0.95. The LV2 plugin is the real Dragonfly
//! Hall bundled under `tests/fixtures/plugins/lv2`, so the test needs no
//! external checkout.

use std::path::PathBuf;
use std::sync::Once;

use domain::ids::{BlockId, ChainId};
use engine::offline::render_chain;
use project::block::{normalize_block_params, AudioBlock, AudioBlockKind, CoreBlock};
use project::chain::Chain;
use project::param::ParameterSet;

const SR: f32 = 48_000.0;
const BLOCK: usize = 256;
const MAX_CORRELATION: f32 = 0.95;

fn init_registry() {
    static INIT: Once = Once::new();
    INIT.call_once(|| {
        lv2::register_builder();
        block_delay::register_natives();
        block_reverb::register_natives();
        plugin_loader::registry::init(
            &PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("tests/fixtures/plugins"),
        );
    });
}

fn one_block_chain(effect_type: &str, model: &str) -> Chain {
    let params = normalize_block_params(effect_type, model, ParameterSet::default())
        .unwrap_or_else(|e| panic!("{model}: default params must normalize: {e}"));
    Chain {
        id: ChainId(format!("issue-938-{model}")),
        description: None,
        instrument: "electric_guitar".into(),
        enabled: true,
        volume: 100.0,
        io_binding_ids: Vec::new(),
        di_output: None,
        loopers: vec![],
        blocks: vec![AudioBlock {
            id: BlockId(format!("issue-938-{model}-block")),
            enabled: true,
            kind: AudioBlockKind::Core(CoreBlock {
                effect_type: effect_type.into(),
                model: model.into(),
                params,
            }),
        }],
    }
}

/// Plucked-note stand-in: decaying noise bursts, identical on both channels.
fn mono_source(seconds: f32) -> Vec<[f32; 2]> {
    let frames = (seconds * SR) as usize;
    let note = (0.5 * SR) as usize;
    let mut seed: u32 = 0x1234_5678;
    (0..frames)
        .map(|n| {
            seed = seed.wrapping_mul(1_664_525).wrapping_add(1_013_904_223);
            let noise = (seed >> 8) as f32 / (1u32 << 24) as f32 * 2.0 - 1.0;
            let age = (n % note) as f32 / SR;
            let s = 0.3 * noise * (-age * 12.0).exp();
            [s, s]
        })
        .collect()
}

fn correlation(frames: &[[f32; 2]]) -> f32 {
    let (mut lr, mut ll, mut rr) = (0.0f64, 0.0f64, 0.0f64);
    for [l, r] in frames {
        lr += (*l as f64) * (*r as f64);
        ll += (*l as f64) * (*l as f64);
        rr += (*r as f64) * (*r as f64);
    }
    (lr / (ll * rr).sqrt()) as f32
}

/// Measured on the tail after the source stops — the effect's own output.
/// While the source plays, the dry signal (identical on both sides) weighs
/// on the correlation by however much the model's default mix keeps; the
/// tail is where a dual-mono build shows up as exactly L == R.
fn assert_spreads(effect_type: &str, model: &str) {
    init_registry();
    let chain = one_block_chain(effect_type, model);
    let source = mono_source(2.0);
    let outcome = render_chain(&chain, SR, &source, BLOCK, SR as usize)
        .unwrap_or_else(|e| panic!("{model}: render failed: {e}"));
    assert!(
        outcome.faulted_blocks.is_empty(),
        "{model}: block faulted, render is a pass-through: {:?}",
        outcome.faulted_blocks
    );
    let tail = &outcome.samples[source.len()..];
    let corr = correlation(tail);
    assert!(
        corr < MAX_CORRELATION,
        "{model}: the tail of a mono source came out with L×R correlation \
         {corr:.3} (1.000 = L == R, whole render {:.3}); a stereo effect must \
         spread it below {MAX_CORRELATION}",
        correlation(&outcome.samples)
    );
}

#[test]
fn lv2_two_in_two_out_reverb_spreads_a_mono_source() {
    assert_spreads("reverb", "lv2_dragonfly_hall");
}

#[test]
fn lv2_one_in_two_out_reverb_spreads_a_mono_source() {
    assert_spreads("reverb", "lv2_caps_plate");
}

#[test]
fn ping_pong_spreads_a_mono_source_through_the_engine() {
    assert_spreads("delay", "ping_pong");
}
