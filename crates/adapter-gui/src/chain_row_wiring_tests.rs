//! Tests pro caminho exato do bug reportado pelo user (issue #440):
//! chain disabled → slider altera volume → toggle enabled → volume
//! volta pra 100. Reproduzir IN-MEMORY (sem Slint, sem YAML) é o que
//! garante que a regressão é pega antes de chegar no app.
//!
//! Os 30 testes V01-V30 em infra-yaml só cobrem YAML round-trip, que
//! NÃO acontece no caminho do user (tudo memory). Esse arquivo cobre
//! o gap.

use super::chain_row_wiring::{apply_chain_volume_change, apply_toggle_chain_enabled};
use super::state::ProjectSession;
use domain::ids::ChainId;
use project::chain::Chain;
use project::project::Project;
use std::path::PathBuf;

fn session_with_disabled_chain(initial_volume: f32) -> ProjectSession {
    ProjectSession {
        project: Project {
            name: None,
            device_settings: Vec::new(),
            chains: vec![Chain {
                id: ChainId("test_chain".into()),
                description: None,
                instrument: "electric_guitar".into(),
                enabled: false,
                volume: initial_volume,
                blocks: Vec::new(),
            }],
        },
        project_path: None,
        config_path: None,
        presets_path: PathBuf::from("./presets"),
    }
}

#[test]
fn apply_chain_volume_change_updates_volume_in_memory() {
    let mut session = session_with_disabled_chain(100.0);
    let id = apply_chain_volume_change(&mut session, 0, 150.0).unwrap();
    assert_eq!(id.0, "test_chain");
    assert_eq!(session.project.chains[0].volume, 150.0);
}

#[test]
fn apply_chain_volume_change_invalid_index_returns_none() {
    let mut session = session_with_disabled_chain(100.0);
    assert!(apply_chain_volume_change(&mut session, 99, 150.0).is_none());
}

#[test]
fn apply_toggle_chain_enabled_flips_enabled() {
    let mut session = session_with_disabled_chain(100.0);
    let (new_enabled, _) = apply_toggle_chain_enabled(&mut session, 0).unwrap();
    assert!(new_enabled);
    assert_eq!(session.project.chains[0].enabled, true);
}

#[test]
fn apply_toggle_chain_enabled_does_not_touch_volume() {
    // Regressão pinada: o handler toggle NUNCA pode mexer em volume.
    let mut session = session_with_disabled_chain(175.0);
    apply_toggle_chain_enabled(&mut session, 0).unwrap();
    assert_eq!(
        session.project.chains[0].volume, 175.0,
        "toggle must not touch volume"
    );
}

#[test]
fn user_scenario_disabled_volume_change_then_enable_preserves_volume() {
    // ANCHOR do bug do user (commit "mesma merda. aumentei o volume na chain..
    // antes de ligar.. liguei e voltou para 100%").
    //
    // 1. Chain está disabled, volume=100 (default).
    // 2. User arrasta slider pra 150 — handler dispara apply_chain_volume_change.
    // 3. User toggla enable — handler dispara apply_toggle_chain_enabled.
    // 4. Volume DEVE continuar 150.
    let mut session = session_with_disabled_chain(100.0);

    // Step 2: slider drag while disabled.
    apply_chain_volume_change(&mut session, 0, 150.0).unwrap();
    assert_eq!(session.project.chains[0].volume, 150.0);
    assert!(!session.project.chains[0].enabled);

    // Step 3: toggle enable.
    let (new_enabled, _) = apply_toggle_chain_enabled(&mut session, 0).unwrap();
    assert!(new_enabled);

    // Step 4: assert volume PERSISTE.
    assert_eq!(
        session.project.chains[0].volume, 150.0,
        "user scenario: volume must survive toggle disabled→enabled"
    );
}

#[test]
fn user_scenario_then_toggle_back_disabled_preserves_volume() {
    // Variação: depois do enable, toggle disable de novo. Volume mantém.
    let mut session = session_with_disabled_chain(100.0);
    apply_chain_volume_change(&mut session, 0, 175.0).unwrap();
    apply_toggle_chain_enabled(&mut session, 0).unwrap(); // → enabled
    apply_toggle_chain_enabled(&mut session, 0).unwrap(); // → disabled
    assert_eq!(session.project.chains[0].volume, 175.0);
    assert!(!session.project.chains[0].enabled);
}

#[test]
fn multiple_chains_independent_volumes_after_toggle() {
    // Setup: 2 chains, ambas disabled, volumes diferentes.
    let mut session = ProjectSession {
        project: Project {
            name: None,
            device_settings: Vec::new(),
            chains: vec![
                Chain {
                    id: ChainId("a".into()),
                    description: None,
                    instrument: "electric_guitar".into(),
                    enabled: false,
                    volume: 50.0,
                    blocks: Vec::new(),
                },
                Chain {
                    id: ChainId("b".into()),
                    description: None,
                    instrument: "electric_guitar".into(),
                    enabled: false,
                    volume: 175.0,
                    blocks: Vec::new(),
                },
            ],
        },
        project_path: None,
        config_path: None,
        presets_path: PathBuf::from("./presets"),
    };
    // User toggla chain 0 → enabled. Chain 1 fica como tá.
    apply_toggle_chain_enabled(&mut session, 0).unwrap();
    assert_eq!(session.project.chains[0].volume, 50.0);
    assert_eq!(session.project.chains[1].volume, 175.0);
    assert!(session.project.chains[0].enabled);
    assert!(!session.project.chains[1].enabled);
}

#[test]
fn volume_change_when_chain_already_enabled_persists() {
    let mut session = ProjectSession {
        project: Project {
            name: None,
            device_settings: Vec::new(),
            chains: vec![Chain {
                id: ChainId("c".into()),
                description: None,
                instrument: "electric_guitar".into(),
                enabled: true,
                volume: 100.0,
                blocks: Vec::new(),
            }],
        },
        project_path: None,
        config_path: None,
        presets_path: PathBuf::from("./presets"),
    };
    apply_chain_volume_change(&mut session, 0, 60.0).unwrap();
    assert_eq!(session.project.chains[0].volume, 60.0);
    assert!(session.project.chains[0].enabled);
}

#[test]
fn rapid_volume_changes_keep_last_value() {
    // Slider arrastando rápido = vários callbacks back-to-back.
    let mut session = session_with_disabled_chain(100.0);
    for v in [120.0, 140.0, 160.0, 180.0, 200.0_f32] {
        apply_chain_volume_change(&mut session, 0, v).unwrap();
    }
    assert_eq!(session.project.chains[0].volume, 200.0);
}

#[test]
fn volume_zero_is_preserved_through_toggle() {
    let mut session = session_with_disabled_chain(100.0);
    apply_chain_volume_change(&mut session, 0, 0.0).unwrap();
    apply_toggle_chain_enabled(&mut session, 0).unwrap();
    assert_eq!(session.project.chains[0].volume, 0.0);
}
