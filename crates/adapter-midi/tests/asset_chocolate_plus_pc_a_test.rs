//! Phase 5 red-first test (issue #548): verify the shipped Chocolate
//! Plus "Program change A" profile loads cleanly through the Phase 2
//! parser and exposes the expected slots in the expected banks.

use adapter_midi::profile::{parse_profile_yaml, MatchExpr};

fn load_asset() -> String {
    let path = std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
        .ancestors()
        .nth(2)
        .expect("repo root")
        .join("assets/midi-profiles/chocolate_plus_program_change_a.yaml");
    std::fs::read_to_string(&path)
        .unwrap_or_else(|e| panic!("read {:?}: {e}", path))
}

#[test]
fn chocolate_plus_pc_a_loads() {
    let yaml = load_asset();
    let profile = parse_profile_yaml(&yaml).expect("must parse cleanly");
    assert!(profile.name.contains("Chocolate"));
    assert_eq!(profile.source.as_deref(), Some("FootCtrlPlus"));
    assert!(
        profile.bindings.len() >= 16,
        "expected at least 16 bindings (banks 1-4 × 4 switches), got {}",
        profile.bindings.len()
    );
}

#[test]
fn chocolate_plus_pc_a_bank_1_is_preset_scene_nav() {
    let yaml = load_asset();
    let profile = parse_profile_yaml(&yaml).expect("parse");

    let expected: &[(u8, &str)] = &[
        (0, "prev_preset"),
        (1, "next_preset"),
        (2, "prev_scene"),
        (3, "next_scene"),
    ];

    for (program, slot) in expected {
        let found = profile.bindings.iter().any(|b| {
            matches!(
                &b.when,
                MatchExpr::ProgramChange { channel: 1, program: Some(p) } if *p == *program
            ) && b.action == *slot
        });
        assert!(
            found,
            "expected PC {} → {} not found in bank 1",
            program, slot
        );
    }
}

#[test]
fn chocolate_plus_pc_a_covers_toggle_tuner_and_mute() {
    let yaml = load_asset();
    let profile = parse_profile_yaml(&yaml).expect("parse");
    let actions: Vec<&str> = profile.bindings.iter().map(|b| b.action.as_str()).collect();
    assert!(
        actions.contains(&"toggle_tuner"),
        "must include toggle_tuner for live use"
    );
    assert!(
        actions.contains(&"toggle_output_mute"),
        "must include toggle_output_mute for live use"
    );
}
