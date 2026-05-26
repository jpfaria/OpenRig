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
        profile.bindings.len() >= 15,
        "expected at least 15 bindings (banks 1-3 × 4 + bank 4 × 3, PC 15 left unbound), got {}",
        profile.bindings.len()
    );
}

#[test]
fn chocolate_plus_pc_a_bank_1_is_chain_nav() {
    let yaml = load_asset();
    let profile = parse_profile_yaml(&yaml).expect("parse");

    let expected: &[(u8, &str)] = &[
        (0, "prev_chain"),
        (1, "toggle_active_chain_enabled"),
        (2, "toggle_compact_view"),
        (3, "next_chain"),
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
fn chocolate_plus_pc_a_bank_3_is_block_pair() {
    let yaml = load_asset();
    let profile = parse_profile_yaml(&yaml).expect("parse");

    let expected: &[(u8, &str)] = &[
        (8, "prev_block_2"),
        (9, "toggle_active_block_enabled"),
        (10, "toggle_active_block_neighbor_enabled"),
        (11, "next_block_2"),
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
            "expected PC {} → {} not found in bank 3",
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
