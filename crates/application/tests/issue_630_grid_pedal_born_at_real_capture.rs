//! Issue #630 (born-default) — a freshly added NAM grid pedal must be born
//! at a REAL capture, never at the per-axis-minimum combination.
//!
//! User's point: the axis values are predefined (e.g. a TS9 `drive` axis or a
//! manifest axis `[10, 20, 30]`), so the default must be a real grid value
//! that maps to an EXISTING capture — never 0-by-default and never a
//! combination that is absent from the capture list.
//!
//! Before the fix, `build_default_block` filled every grid axis independently
//! with its first declared value (`grid_parameter_to_spec` default). For a
//! multi-axis grid that per-axis-min combination can be a cell that does NOT
//! exist in the manifest. The `nam_ts9_grid` fixture is built exactly so:
//!   axes: drive [0, 5], tone [6, 3], level [6]
//!   per-axis-first = (drive=0, tone=6, level=6) -> NOT a declared capture
//!   first declared capture = (drive=0, tone=3, level=6)
//! So the born default must equal (drive=0, tone=3, level=6), the first real
//! capture, not the invalid per-axis-min combination.

use std::path::PathBuf;
use std::sync::Once;

use application::block_factory::{build_default_block, resolve_effect_type_for_model};
use domain::ids::BlockId;
use plugin_loader::manifest::ParameterValue as ManifestValue;
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

/// The born-default `ParameterSet` of a multi-axis grid pedal must equal one
/// of the manifest's captures (issue #630). It must NOT be the per-axis-min
/// combination when that combination is not a declared capture.
#[test]
fn grid_pedal_born_default_equals_a_real_capture() {
    init_plugins();
    let pkg = plugin_loader::registry::find("nam_ts9_grid")
        .expect("fixture nam_ts9_grid must be discoverable");
    let captures = match &pkg.manifest.backend {
        plugin_loader::manifest::Backend::Nam { captures, .. } => captures.clone(),
        other => panic!("fixture must be a NAM grid backend, got {other:?}"),
    };

    let effect_type = resolve_effect_type_for_model("nam_ts9_grid")
        .expect("effect type for the grid pedal must resolve");
    let block = build_default_block(BlockId("blk".into()), &effect_type, "nam_ts9_grid")
        .expect("building a default NAM grid pedal must succeed");
    // A grid-backed package may materialise as either a `Nam` block or a
    // `Core` block depending on its manifest `type:`; both carry the grid
    // `ParameterSet`. Read it from whichever variant the factory produced.
    let params = match block.kind {
        AudioBlockKind::Nam(nam) => nam.params,
        AudioBlockKind::Core(core) => core.params,
        other => panic!("expected a grid-backed block, got {}", other.label()),
    };

    // The born grid-axis values, read back from the default ParameterSet.
    let drive = params.get_f32("drive").expect("drive must be set");
    let tone = params.get_f32("tone").expect("tone must be set");
    let level = params.get_f32("level").expect("level must be set");

    // Assert the (drive, tone, level) triple matches SOME declared capture.
    let matches_a_capture = captures.iter().any(|capture| {
        let cap_axis = |name: &str| match capture.values.get(name) {
            Some(ManifestValue::Number(n)) => Some(*n as f32),
            _ => None,
        };
        cap_axis("drive") == Some(drive)
            && cap_axis("tone") == Some(tone)
            && cap_axis("level") == Some(level)
    });
    assert!(
        matches_a_capture,
        "issue #630: the born default (drive={drive}, tone={tone}, level={level}) must equal a \
         real capture from the manifest — never the per-axis-min combination that is absent from \
         the capture list"
    );

    // Stronger, deterministic contract: born at the FIRST declared capture
    // (drive=0, tone=3, level=6), since the manifest lists captures in order.
    assert_eq!(drive, 0.0, "first capture's drive");
    assert_eq!(tone, 3.0, "first capture's tone (NOT the per-axis-first 6)");
    assert_eq!(level, 6.0, "first capture's level");
}
