//! Issue #675 — a NAM block born from a capture must seed the noise-gate
//! knobs from the manifest, so a high-gain capture ships the gate
//! pre-configured in the USER-VISIBLE knobs (editable, persisted) — not as
//! a hidden load-time default. Mirrors the `output_db` seeding (#655).

use std::path::PathBuf;
use std::sync::Once;

use application::block_factory::{build_default_block, resolve_effect_type_for_model};
use domain::ids::BlockId;
use project::block::AudioBlockKind;

fn fixture_plugins_root() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("tests/fixtures/plugins")
}

fn init_plugins() {
    static INIT: Once = Once::new();
    INIT.call_once(|| {
        plugin_loader::registry::init(&fixture_plugins_root());
    });
}

#[test]
fn nam_block_seeds_noise_gate_knobs_from_manifest() {
    init_plugins();
    let effect_type =
        resolve_effect_type_for_model("nam_ts9_grid").expect("effect type for the grid pedal");
    let block = build_default_block(BlockId("blk".into()), &effect_type, "nam_ts9_grid")
        .expect("building the default NAM block must succeed");
    let params = match block.kind {
        AudioBlockKind::Nam(nam) => nam.params,
        AudioBlockKind::Core(core) => core.params,
        other => panic!("expected a grid-backed block, got {}", other.label()),
    };

    // Manifest-level `enabled: true` seeds the knob; the born (first) capture
    // overrides only the threshold (-55), inheriting `enabled` from manifest.
    assert_eq!(
        params.get_bool("noise_gate.enabled"),
        Some(true),
        "gate `enabled` must be seeded from the manifest into the user-visible knob"
    );
    assert_eq!(
        params.get_f32("noise_gate.threshold_db"),
        Some(-55.0),
        "`threshold_db` must be seeded from the per-capture override into the knob"
    );
}
