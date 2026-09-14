//! Issue #938 — a VST3 block addressed by the id `openrig://plugins` lists
//! (the package manifest id, e.g. `vst3_room_reverb`) must build and play,
//! not bypass with "VST3 plugin not found in catalog". The catalog only knew
//! its own `vst3:{bundle}:{class}` id, so every id the listing handed out
//! failed.
//!
//! Real-plugin test, env-gated on `OPENRIG_TEST_VST3_DIR` (the plugins `vst3/`
//! dir, e.g. `<OpenRig-plugins>/plugins/source/vst3`) like the #776 battery —
//! the RoomReverb bundle is 19 MB, too large to vendor. It skips loudly when
//! the variable is unset. Run with:
//!   OPENRIG_TEST_VST3_DIR=<.../vst3> cargo test -p engine \
//!     --test issue_938_vst3_package_id_renders -- --test-threads=1

use std::path::PathBuf;

use domain::ids::{BlockId, ChainId};
use engine::offline::render_chain;
use project::block::{normalize_block_params, AudioBlock, AudioBlockKind, CoreBlock};
use project::chain::Chain;
use project::param::ParameterSet;

const SR: f32 = 48_000.0;
const PACKAGE_ID: &str = "vst3_room_reverb";

/// The plugins `vst3/` dir, with the catalog and the package registry both
/// initialised from it. `None` when the variable is unset.
fn init_from_env() -> Option<PathBuf> {
    let vst3_dir = PathBuf::from(std::env::var_os("OPENRIG_TEST_VST3_DIR")?);
    let plugins_root = vst3_dir.parent()?.to_path_buf();
    vst3_host::init_vst3_catalog(SR as f64, &[vst3_dir.clone()]);
    plugin_loader::registry::init(&plugins_root);
    Some(vst3_dir)
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

#[test]
fn vst3_block_named_by_its_package_id_renders_in_stereo() {
    if init_from_env().is_none() {
        eprintln!("[#938 VST3] OPENRIG_TEST_VST3_DIR not set — skipping");
        return;
    }
    let params = normalize_block_params(
        block_core::EFFECT_TYPE_VST3,
        PACKAGE_ID,
        ParameterSet::default(),
    )
    .unwrap_or_else(|e| panic!("{PACKAGE_ID}: schema must resolve by package id: {e}"));
    let chain = Chain {
        id: ChainId("issue-938-vst3".into()),
        description: None,
        instrument: "electric_guitar".into(),
        enabled: true,
        volume: 100.0,
        io_binding_ids: Vec::new(),
        di_output: None,
        loopers: vec![],
        blocks: vec![AudioBlock {
            id: BlockId("issue-938-vst3-block".into()),
            enabled: true,
            kind: AudioBlockKind::Core(CoreBlock {
                effect_type: block_core::EFFECT_TYPE_VST3.into(),
                model: PACKAGE_ID.into(),
                params,
            }),
        }],
    };
    let source: Vec<[f32; 2]> = (0..(SR as usize))
        .map(|n| {
            let age = (n % 24_000) as f32 / SR;
            let s = 0.3
                * (2.0 * std::f32::consts::PI * 220.0 * n as f32 / SR).sin()
                * (-age * 12.0).exp();
            [s, s]
        })
        .collect();

    let outcome = render_chain(&chain, SR, &source, 512, SR as usize)
        .unwrap_or_else(|e| panic!("{PACKAGE_ID}: render failed: {e}"));

    assert!(
        outcome.faulted_blocks.is_empty(),
        "{PACKAGE_ID}: block faulted, render is a pass-through: {:?}",
        outcome.faulted_blocks
    );
    let tail = &outcome.samples[source.len()..];
    let corr = correlation(tail);
    assert!(
        corr < 0.95,
        "{PACKAGE_ID}: the reverb tail of a mono source has L×R correlation {corr:.3}"
    );
}
