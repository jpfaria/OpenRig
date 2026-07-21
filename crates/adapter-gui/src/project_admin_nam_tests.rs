//! Admin persistence: NAM/install + command-driven tests (issue #792 split
//! from project_admin_persistence_tests.rs). Shares session/chain fixtures via super.

use application::command::Command;
use application::dispatcher::CommandDispatcher;
use domain::ids::BlockId;
use domain::value_objects::ParameterValue;
use project::block::{AudioBlock, AudioBlockKind, CoreBlock};
use project::param::ParameterSet;

use super::project_admin_persistence_tests::{
    add_chain, chain_count, chain_descriptions, find_chain, gain_block, owner_plugins_root, Sandbox,
};

// ────────────────────────────────────────────────────────────────────

// ────────────────────────────────────────────────────────────────────
// 4. Block parameters

#[test]
fn set_block_parameter_number_persists() {
    let s = Sandbox::new();
    let session = s.new_session();
    let chain_id = add_chain(&session, "X");
    let pos = session.project.borrow().chains[0].blocks.len() - 1;
    session
        .dispatcher
        .dispatch(Command::InsertPrebuiltBlock {
            chain: chain_id.clone(),
            block: gain_block("g1", 0.0),
            position: pos,
        })
        .expect("Insert");

    session
        .dispatcher
        .dispatch(Command::SetBlockParameterNumber {
            chain: chain_id.clone(),
            block: BlockId("g1".into()),
            path: "drive".into(),
            value: 42.0,
        })
        .expect("SetBlockParameterNumber");
    s.save(&session);

    let reloaded = s.reload();
    let v = reloaded
        .project
        .borrow()
        .chains
        .iter()
        .find(|c| c.description.as_deref() == Some("X"))
        .and_then(|c| {
            c.blocks
                .iter()
                .find(|b| b.id.0 == "g1")
                .and_then(|b| match &b.kind {
                    AudioBlockKind::Core(cb) => cb.params.get_f32("drive"),
                    _ => None,
                })
        })
        .expect("drive param present");
    assert!(
        (v - 42.0).abs() < 1e-3,
        "SetBlockParameterNumber did not persist (got {v})"
    );
}

#[test]
fn set_block_parameter_bool_persists() {
    let s = Sandbox::new();
    let session = s.new_session();
    let chain_id = add_chain(&session, "X");
    let pos = session.project.borrow().chains[0].blocks.len() - 1;
    let mut params = ParameterSet::default();
    params.insert("active", ParameterValue::Bool(false));
    session
        .dispatcher
        .dispatch(Command::InsertPrebuiltBlock {
            chain: chain_id.clone(),
            block: AudioBlock {
                id: BlockId("g1".into()),
                enabled: true,
                kind: AudioBlockKind::Core(CoreBlock {
                    effect_type: "gain".into(),
                    model: "standard".into(),
                    params,
                }),
            },
            position: pos,
        })
        .expect("Insert");

    session
        .dispatcher
        .dispatch(Command::SetBlockParameterBool {
            chain: chain_id.clone(),
            block: BlockId("g1".into()),
            path: "active".into(),
            value: true,
        })
        .expect("SetBlockParameterBool");
    s.save(&session);

    let reloaded = s.reload();
    let v = reloaded
        .project
        .borrow()
        .chains
        .iter()
        .find(|c| c.description.as_deref() == Some("X"))
        .and_then(|c| {
            c.blocks
                .iter()
                .find(|b| b.id.0 == "g1")
                .and_then(|b| match &b.kind {
                    AudioBlockKind::Core(cb) => cb.params.get_bool("active"),
                    _ => None,
                })
        })
        .expect("active param present");
    assert!(v, "SetBlockParameterBool did not persist (got {v})");
}

#[test]
fn set_block_parameter_text_persists() {
    let s = Sandbox::new();
    let session = s.new_session();
    let chain_id = add_chain(&session, "X");
    let pos = session.project.borrow().chains[0].blocks.len() - 1;
    let mut params = ParameterSet::default();
    params.insert("label", ParameterValue::String(String::new()));
    session
        .dispatcher
        .dispatch(Command::InsertPrebuiltBlock {
            chain: chain_id.clone(),
            block: AudioBlock {
                id: BlockId("g1".into()),
                enabled: true,
                kind: AudioBlockKind::Core(CoreBlock {
                    effect_type: "gain".into(),
                    model: "standard".into(),
                    params,
                }),
            },
            position: pos,
        })
        .expect("Insert");

    session
        .dispatcher
        .dispatch(Command::SetBlockParameterText {
            chain: chain_id.clone(),
            block: BlockId("g1".into()),
            path: "label".into(),
            value: "HELLO".into(),
        })
        .expect("SetBlockParameterText");
    s.save(&session);

    let reloaded = s.reload();
    let v = reloaded
        .project
        .borrow()
        .chains
        .iter()
        .find(|c| c.description.as_deref() == Some("X"))
        .and_then(|c| {
            c.blocks
                .iter()
                .find(|b| b.id.0 == "g1")
                .and_then(|b| match &b.kind {
                    AudioBlockKind::Core(cb) => cb.params.get_string("label").map(String::from),
                    _ => None,
                })
        })
        .expect("label param present");
    assert_eq!(v, "HELLO", "SetBlockParameterText did not persist");
}

// ────────────────────────────────────────────────────────────────────
// 5. Multi-chain / multi-block integration
// ────────────────────────────────────────────────────────────────────

#[test]
fn full_admin_sequence_round_trips() {
    let s = Sandbox::new();
    let session = s.new_session();

    session
        .dispatcher
        .dispatch(Command::UpdateProjectName {
            name: "STAGE".into(),
        })
        .expect("name");
    let c1 = add_chain(&session, "GUITAR");
    let c2 = add_chain(&session, "BASS");
    let pos = session.project.borrow().chains[0].blocks.len() - 1;
    session
        .dispatcher
        .dispatch(Command::InsertPrebuiltBlock {
            chain: c1.clone(),
            block: gain_block("g1", 25.0),
            position: pos,
        })
        .expect("insert g1");
    session
        .dispatcher
        .dispatch(Command::SetChainVolume {
            chain: c1.clone(),
            value: 75.0,
        })
        .expect("vol");
    session
        .dispatcher
        .dispatch(Command::MoveChainUp { chain: c2.clone() })
        .expect("move up");
    s.save(&session);

    let reloaded = s.reload();
    let p = reloaded.project.borrow();
    assert_eq!(p.name.as_deref(), Some("STAGE"));
    assert_eq!(
        p.chains
            .iter()
            .map(|c| c.description.clone())
            .collect::<Vec<_>>(),
        vec![Some("BASS".to_string()), Some("GUITAR".to_string())],
    );
    let guitar = p
        .chains
        .iter()
        .find(|c| c.description.as_deref() == Some("GUITAR"))
        .expect("GUITAR present");
    assert!((guitar.volume - 75.0).abs() < 1e-3);
    assert!(
        guitar.blocks.iter().any(|b| b.id.0 == "g1"),
        "block g1 persisted on GUITAR (got {:?})",
        guitar
            .blocks
            .iter()
            .map(|b| b.id.0.clone())
            .collect::<Vec<_>>(),
    );
}

// ────────────────────────────────────────────────────────────────────
// 6. Empty-state sanity (regression of the bug)
// ────────────────────────────────────────────────────────────────────

#[test]
fn empty_project_then_add_chain_then_reload_keeps_chain() {
    let s = Sandbox::new();
    let session = s.new_session();
    // Save the empty project first (mimic "user creates, doesn't add a chain, saves").
    s.save(&session);

    // Reopen, add a chain, save, reopen.
    let session = s.reload();
    add_chain(&session, "FIRST");
    s.save(&session);

    let reloaded = s.reload();
    assert_eq!(
        chain_descriptions(&reloaded),
        vec![Some("FIRST".to_string())],
        "chain added to a re-opened empty project did not persist"
    );
}

#[test]
fn delete_only_chain_then_add_new_one_round_trips() {
    let s = Sandbox::new();
    let session = s.new_session();
    let id = add_chain(&session, "OLD");
    s.save(&session);

    let session = s.reload();
    let id = session
        .project
        .borrow()
        .chains
        .iter()
        .find(|c| c.description.as_deref() == Some("OLD"))
        .map(|c| c.id.clone())
        .unwrap_or(id);
    session
        .dispatcher
        .dispatch(Command::RemoveChain { chain: id })
        .expect("RemoveChain");
    add_chain(&session, "NEW");
    s.save(&session);

    let reloaded = s.reload();
    assert_eq!(
        chain_descriptions(&reloaded),
        vec![Some("NEW".to_string())],
        "after deleting the only chain and adding a new one, the new chain disappeared"
    );
}

#[test]
fn chain_count_is_stable_across_reload() {
    let s = Sandbox::new();
    let session = s.new_session();
    for n in 0..3 {
        add_chain(&session, &format!("C{n}"));
    }
    let before = chain_count(&session);
    s.save(&session);
    let after = chain_count(&s.reload());
    assert_eq!(before, after, "chain count drifted across reload");
}

// Sanity: the `find_chain` helper is exercised in other tests, but we
// pin it here in case it gets dropped — fails compile if removed by mistake.
#[test]
fn find_chain_helper_locates_added_chain() {
    let s = Sandbox::new();
    let session = s.new_session();
    add_chain(&session, "GUITAR");
    assert!(find_chain(&session, "GUITAR").is_some());
}

// ────────────────────────────────────────────────────────────────────
// Issue #606 — NAM-backed gain model survives load
// ────────────────────────────────────────────────────────────────────
//
// User log: `[ERROR adapter_gui::helpers] block '…': unsupported gain
// model 'nam_lovepedal_eternity_burst'`. The catalog files NAM gain
// pedals (manifest `type: gain_pedal`, `backend: nam`) under the "gain"
// family, so a slot holding one is a `Core { effect_type: "gain",
// model: "nam_…" }`. The engine's offline `render_chain` builds such a
// block fine (it consults the plugin catalog), but the GUI load path
// validates "gain" blocks against the NATIVE block-gain registry only and
// drops the model as unsupported — losing the user's block on reload.
//
// Repro uses `nam_maxon_od808` from OpenRig-plugins (same shape as the
// user's `nam_lovepedal_eternity_burst`). The catalog is initialised from
// the real plugin tree AND the config points at it, so the package is
// unambiguously known — the only way the block disappears is the
// native-only validation.
fn nam_gain_block(id: &str) -> AudioBlock {
    AudioBlock {
        id: BlockId(id.into()),
        enabled: true,
        kind: AudioBlockKind::Core(CoreBlock {
            effect_type: "gain".into(),
            model: "nam_maxon_od808".into(),
            params: ParameterSet::default(),
        }),
    }
}

#[test]
fn issue_606_nam_backed_gain_block_survives_load() {
    let Some(plugins_root) = owner_plugins_root() else {
        eprintln!(
            "[#606] SKIPPED — set OPENRIG_OWNER_PLUGINS=<OpenRig-plugins/plugins/source> to run"
        );
        return;
    };
    // Populate the process-global catalog so `nam_maxon_od808` is a known
    // disk-package gain model — isolates the routing bug from any
    // catalog-not-loaded effect.
    plugin_loader::registry::init_many(std::slice::from_ref(&plugins_root));

    let s = Sandbox::new();
    let session = s.new_session();
    // Point the config at the same real plugin tree (mirrors the user's
    // configured setup) so the load path can resolve the package too.
    std::fs::write(
        &s.cfg,
        format!("plugins_root: {}\n", plugins_root.display()),
    )
    .expect("write config with plugins_root");

    let chain_id = add_chain(&session, "X");
    let pos = session.project.borrow().chains[0].blocks.len() - 1;
    session
        .dispatcher
        .dispatch(Command::InsertPrebuiltBlock {
            chain: chain_id.clone(),
            block: nam_gain_block("nam_od"),
            position: pos,
        })
        .expect("InsertPrebuiltBlock");
    s.save(&session);

    let reloaded = s.reload();
    let survived = reloaded
        .project
        .borrow()
        .chains
        .iter()
        .find(|c| c.description.as_deref() == Some("X"))
        .map(|c| {
            c.blocks.iter().any(
                |b| matches!(&b.kind, AudioBlockKind::Core(cb) if cb.model == "nam_maxon_od808"),
            )
        })
        .unwrap_or(false);
    assert!(
        survived,
        "BUG #606: NAM-backed gain block 'nam_maxon_od808' was dropped on load \
         (logged as 'unsupported gain model') — the load path validated the \
         model against the native block-gain registry instead of the plugin \
         catalog"
    );
}

// A gain block whose `nam_` pack is NOT installed in the catalog.
fn uninstalled_nam_gain_block(id: &str) -> AudioBlock {
    AudioBlock {
        id: BlockId(id.into()),
        enabled: true,
        kind: AudioBlockKind::Core(CoreBlock {
            effect_type: "gain".into(),
            model: "nam_uninstalled_pedal_for_issue_606".into(),
            params: ParameterSet::default(),
        }),
    }
}

#[test]
fn issue_606_uninstalled_model_block_is_disabled_on_load() {
    let Some(plugins_root) = owner_plugins_root() else {
        eprintln!(
            "[#606] SKIPPED — set OPENRIG_OWNER_PLUGINS=<OpenRig-plugins/plugins/source> to run"
        );
        return;
    };
    // Catalog loaded, but the block's `nam_` pack is deliberately absent.
    plugin_loader::registry::init_many(std::slice::from_ref(&plugins_root));

    let s = Sandbox::new();
    let session = s.new_session();
    std::fs::write(
        &s.cfg,
        format!("plugins_root: {}\n", plugins_root.display()),
    )
    .expect("write config with plugins_root");

    let chain_id = add_chain(&session, "X");
    let pos = session.project.borrow().chains[0].blocks.len() - 1;
    session
        .dispatcher
        .dispatch(Command::InsertPrebuiltBlock {
            chain: chain_id.clone(),
            block: uninstalled_nam_gain_block("ghost"),
            position: pos,
        })
        .expect("InsertPrebuiltBlock");
    s.save(&session);

    let reloaded = s.reload();
    let proj = reloaded.project.borrow();
    let block = proj
        .chains
        .iter()
        .flat_map(|c| c.blocks.iter())
        .find(|b| b.id.0 == "ghost")
        .expect("the block must be preserved on load, not dropped");
    assert!(
        !block.enabled,
        "BUG #606: a block whose model is uninstalled must be DISABLED on load \
         so the chain keeps playing instead of leaving a silently-faulted 'on' \
         pedal"
    );
}
