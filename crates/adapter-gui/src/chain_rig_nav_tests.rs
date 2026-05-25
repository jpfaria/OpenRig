use super::{preset_slot_at, rig_nav_rows, RigNavRow};
use engine::rig_runtime::{rig_to_legacy_project, switch_and_project_input};
use project::block::InputEntry;
use project::chain::ChainInputMode;
use project::rig::{RigInput, RigPreset, RigProject};
use std::collections::{BTreeMap, BTreeSet};

fn rig() -> RigProject {
    let mut presets = BTreeMap::new();
    for n in ["clean", "drive", "lead"] {
        presets.insert(
            n.to_string(),
            RigPreset {
                id: String::new(),
                name: None,
                blocks: vec![],
                scene_params: vec![],
                scenes: BTreeMap::new(),
                volume: 100.0,
            },
        );
    }
    let mut inputs = BTreeMap::new();
    inputs.insert(
        "input-1".to_string(),
        RigInput {
            label: None,
            sources: vec![InputEntry {
                device_id: domain::ids::DeviceId("d".into()),
                mode: ChainInputMode::Mono,
                channels: vec![0],
            }],
            bank: BTreeMap::from([
                (1, "clean".to_string()),
                (2, "drive".to_string()),
                (3, "lead".to_string()),
            ]),
            active_preset: 2,
            active_scene: 4,
            routing: vec![],
        },
    );
    RigProject {
        name: Some("p".into()),
        inputs,
        outputs: BTreeMap::new(),
        presets,
        midi: None,
        chain_order: Vec::new(),
    }
}

#[test]
fn nav_row_exposes_bank_active_and_scene_aligned_to_chains() {
    let r = rig();
    let proj = rig_to_legacy_project(&r, &BTreeSet::new());
    let rows = rig_nav_rows(&r, &proj);

    assert_eq!(rows.len(), proj.chains.len(), "one row per chain, aligned");
    let row = &rows[0];
    assert_eq!(row.input, "input-1");
    assert_eq!(row.preset_slots, vec![1, 2, 3]);
    // #436: presets without a name show the humanized pool key.
    assert_eq!(row.preset_labels, vec!["Clean", "Drive", "Lead"]);
    assert_eq!(row.active_index, 1, "active_preset 2 → index 1");
    assert_eq!(row.scene, 4);
}

#[test]
fn switch_then_nav_reflects_new_active_preset_and_scene() {
    // The exact round-trip the GUI wiring performs, pure (no AppWindow):
    // switch_and_project_input mutates the rig, the chains are
    // re-projected, and rig_nav_rows must report the new active state.
    let mut r = rig(); // input-1: bank {1 clean, 2 drive, 3 lead}, active 2, scene 4
    let before = rig_nav_rows(&r, &rig_to_legacy_project(&r, &BTreeSet::new()));
    assert_eq!(before[0].active_index, 1, "active preset 2 → index 1");
    assert_eq!(before[0].scene, 4);

    let chain =
        switch_and_project_input(&mut r, "input-1", Some(3), Some(7)).expect("rebuilt chain");
    assert_eq!(chain.id.0, "rig:input-1");

    let after = rig_nav_rows(&r, &rig_to_legacy_project(&r, &BTreeSet::new()));
    assert_eq!(after[0].active_index, 2, "active preset 3 → index 2");
    assert_eq!(after[0].scene, 7);
}

#[test]
fn preset_slot_at_maps_combobox_position_to_real_bank_key() {
    // The ComboBox hands back a POSITIONAL index into preset_labels;
    // switch_and_project_input wants the bank KEY. With a non-1-based or
    // sparse bank (the shape "+" produces: max+1) the two diverge, so
    // the position must be translated through the same ordering
    // rig_nav_rows uses (bank.keys() ascending).
    let mut r = rig();
    // Make the bank sparse like an added preset would: {1,2,3} -> add 7.
    r.presets.insert(
        "added".to_string(),
        RigPreset {
            id: String::new(),
            name: None,
            blocks: vec![],
            scene_params: vec![],
            scenes: BTreeMap::new(),
            volume: 100.0,
        },
    );
    r.inputs
        .get_mut("input-1")
        .unwrap()
        .bank
        .insert(7, "added".to_string());

    assert_eq!(preset_slot_at(&r, "input-1", 0), Some(1));
    assert_eq!(preset_slot_at(&r, "input-1", 2), Some(3));
    assert_eq!(
        preset_slot_at(&r, "input-1", 3),
        Some(7),
        "position 3 → key 7"
    );
    assert_eq!(preset_slot_at(&r, "input-1", 4), None, "out of range");
    assert_eq!(preset_slot_at(&r, "missing", 0), None, "unknown input");

    // The position the GUI would send for the added preset must activate
    // exactly that preset (not a key-vs-position off-by-one mismatch).
    let key = preset_slot_at(&r, "input-1", 3).unwrap();
    switch_and_project_input(&mut r, "input-1", Some(key), None).expect("rebuilt");
    let rows = rig_nav_rows(&r, &rig_to_legacy_project(&r, &BTreeSet::new()));
    assert_eq!(rows[0].active_index, 3, "added preset is now active");
    assert_eq!(rows[0].preset_labels[3], "Added");
}

// User repro (#436): clicking a scene gives NO save option. The dirty
// check compares `serialize_project(session.project)` (the projected
// LEGACY Project) before/after. Switching active_scene must register as
// a change there — otherwise Save never lights up and the selected
// scene can't be persisted. With a scene that projects identically the
// snapshot is byte-equal ⇒ not dirty ⇒ "não me deu a opção de salvar".
#[test]
fn switching_scene_changes_the_dirty_snapshot() {
    let mut r = rig(); // input-1 active_scene 4
    r.add_scene_to_input("input-1"); // scene exists; active moves
    r.inputs.get_mut("input-1").unwrap().active_scene = 1;

    // The dirty seam the GUI actually uses for a rig session.
    let snap = |r: &RigProject| {
        crate::project_ops::dirty_snapshot(&rig_to_legacy_project(r, &BTreeSet::new()), Some(r))
            .expect("snapshot")
    };
    let before = snap(&r);

    // The user clicks scene 2.
    r.inputs.get_mut("input-1").unwrap().active_scene = 2;
    let after = snap(&r);

    assert_ne!(
        before, after,
        "switching scene must change the dirty snapshot so Save is offered"
    );
}

// User repro (#436): deleting a chain didn't update the view — it came
// back because the rig session re-projects every RigInput. Removing the
// RigInput must make the projected chain disappear and stay gone.
#[test]
fn removing_a_rig_input_drops_its_chain_from_the_projection() {
    let mut r = rig(); // input-1 → one chain "rig:input-1"
    assert_eq!(
        rig_to_legacy_project(&r, &BTreeSet::new()).chains.len(),
        1,
        "one chain before delete"
    );

    assert!(r.remove_input("input-1"), "input removed");

    let after = rig_to_legacy_project(&r, &BTreeSet::new());
    assert!(
        !after.chains.iter().any(|c| c.id.0 == "rig:input-1"),
        "deleted chain must not re-appear on re-projection"
    );
    assert_eq!(after.chains.len(), 0, "no chains left");
}

// User repro (#436): open the guitar input, ADD a 2nd capture source
// (device + ch1), SAVE, reopen → the input must now have BOTH sources.
// Mirrors the rig save path (the Input block carries input.sources;
// edit it on the synthetic chain, run sync_synthetic_into_rig, reopen).
#[test]
fn adding_a_capture_source_to_a_rig_input_persists_on_save() {
    use project::block::{AudioBlockKind, InputEntry};
    use project::chain::ChainInputMode;

    let mut r = rig(); // input-1 has one source (scarlett? no: see rig())
                       // The rig() helper builds input-1 with a single mono source on "d".
    let before = r.inputs["input-1"].sources.len();
    assert_eq!(before, 1, "starts with one capture source");

    // Step: project, then the user adds device "scarlett" ch1 to the
    // input — on the synthetic chain that is a 2nd entry in the Input
    // block (entries == input.sources).
    let mut proj = rig_to_legacy_project(&r, &BTreeSet::new());
    for c in proj.chains.iter_mut().filter(|c| c.id.0 == "rig:input-1") {
        for b in c.blocks.iter_mut() {
            if let AudioBlockKind::Input(ib) = &mut b.kind {
                ib.entries.push(InputEntry {
                    device_id: domain::ids::DeviceId("scarlett".into()),
                    mode: ChainInputMode::Mono,
                    channels: vec![1],
                });
            }
        }
    }

    // Save → the rig persistence path the GUI runs.
    super::sync_synthetic_into_rig(&mut r, &proj);

    // Reopen → the input must carry BOTH sources.
    let reopened = rig_to_legacy_project(&r, &BTreeSet::new());
    let entries = reopened.chains[0]
        .blocks
        .iter()
        .find_map(|b| match &b.kind {
            AudioBlockKind::Input(ib) => Some(ib.entries.len()),
            _ => None,
        })
        .expect("input block");
    assert_eq!(
        (r.inputs["input-1"].sources.len(), entries),
        (2, 2),
        "added capture source must persist into RigInput.sources"
    );
}

// User repro (#436), steps 3–7: select "New Preset 2" (clone of the
// guitar preset), LOAD a saved preset over it (different blocks), SAVE.
// Reopen → "New Preset 2" must hold the LOADED blocks, not the guitar's.
// Mirrors the real path: load replaces the synthetic chain's processing
// blocks; save runs sync_synthetic_into_rig; reopen projects the rig.
#[test]
fn loading_a_preset_over_a_rig_slot_persists_its_blocks_on_save() {
    use domain::ids::BlockId;
    use project::block::{AudioBlock, AudioBlockKind, CoreBlock};
    use project::param::ParameterSet;

    let blk = |id: &str| AudioBlock {
        id: BlockId(id.into()),
        enabled: true,
        kind: AudioBlockKind::Core(CoreBlock {
            effect_type: "gain".into(),
            model: "volume".into(),
            params: ParameterSet::default(),
        }),
    };
    let mut r = rig();
    // The guitar preset has its own blocks; "New Preset 2" is a clone.
    r.presets.get_mut("clean").unwrap().blocks = vec![blk("G1"), blk("G2")];
    r.inputs.get_mut("input-1").unwrap().active_preset = 1; // clean (guitar)
    let slot = r.add_preset_to_input("input-1").expect("New Preset added");
    let new_name = r.inputs["input-1"].bank[&slot].clone();

    // Step 4: load a saved preset OVER the active slot — the synthetic
    // chain's processing blocks become the loaded preset's (different).
    let mut proj = rig_to_legacy_project(&r, &BTreeSet::new());
    for c in proj.chains.iter_mut().filter(|c| c.id.0 == "rig:input-1") {
        c.blocks
            .retain(|b| matches!(b.kind, AudioBlockKind::Input(_) | AudioBlockKind::Output(_)));
        c.blocks.push(blk("LOADED"));
    }

    // Step 5: save → the rig persistence path the GUI runs.
    super::sync_synthetic_into_rig(&mut r, &proj);

    // Step 6–7: reopen → project the rig; the slot must show LOADED.
    let reopened = rig_to_legacy_project(&r, &BTreeSet::new());
    let ids: Vec<String> = reopened.chains[0]
        .blocks
        .iter()
        .filter_map(|b| match &b.kind {
            AudioBlockKind::Core(_) => Some(b.id.0.clone()),
            _ => None,
        })
        .collect();
    assert_eq!(
        ids,
        vec!["LOADED"],
        "'{new_name}' must hold the LOADED preset, not the guitar blocks \
         (got {ids:?})"
    );
}

// What the user demanded: a unit test proving that switching the preset
// SELECT actually changes the projected chain. clean → block "A",
// drive → block "B"; picking the other ComboBox row must swap the
// processing block, and picking back must restore it.
#[test]
fn switching_preset_select_changes_projected_chain_blocks() {
    use domain::ids::BlockId;
    use project::block::{AudioBlock, AudioBlockKind, CoreBlock};
    use project::param::ParameterSet;

    let blk = |id: &str| AudioBlock {
        id: BlockId(id.into()),
        enabled: true,
        kind: AudioBlockKind::Core(CoreBlock {
            effect_type: "gain".into(),
            model: "volume".into(),
            params: ParameterSet::default(),
        }),
    };
    let mut r = rig(); // bank {1 clean, 2 drive, 3 lead}, active 2
    r.presets.get_mut("clean").unwrap().blocks = vec![blk("A")];
    r.presets.get_mut("drive").unwrap().blocks = vec![blk("B")];
    r.inputs.get_mut("input-1").unwrap().active_preset = 1; // clean

    let processing_ids = |r: &RigProject| {
        switch_and_project_input(&mut r.clone(), "input-1", None, None)
            .expect("chain")
            .blocks
            .iter()
            .filter_map(|b| match &b.kind {
                AudioBlockKind::Core(_) => Some(b.id.0.clone()),
                _ => None,
            })
            .collect::<Vec<_>>()
    };
    assert_eq!(processing_ids(&r), vec!["A"], "clean projects block A");

    // User picks ComboBox position 1 ("drive"): position → key → switch.
    let key = preset_slot_at(&r, "input-1", 1).expect("slot");
    switch_and_project_input(&mut r, "input-1", Some(key), None).expect("switched");
    assert_eq!(processing_ids(&r), vec!["B"], "switching to drive shows B");

    // Pick back to position 0 ("clean").
    let key0 = preset_slot_at(&r, "input-1", 0).expect("slot");
    switch_and_project_input(&mut r, "input-1", Some(key0), None).expect("switched");
    assert_eq!(processing_ids(&r), vec!["A"], "switching back restores A");
}

// Adding a preset must give an INDEPENDENT copy: editing the new
// (active) preset and syncing back must not mutate the source preset —
// otherwise saving "the new preset" would corrupt the old one.
// A new preset is born FRESH (no blocks). When the user adds blocks
// to it and edits them, the source preset must stay untouched — the
// two presets must be fully independent snapshots even when both end
// up holding similar content.
#[test]
fn editing_new_preset_leaves_source_preset_untouched() {
    use domain::ids::BlockId;
    use domain::value_objects::ParameterValue;
    use project::block::{AudioBlock, AudioBlockKind, CoreBlock};
    use project::param::ParameterSet;

    let mut params = ParameterSet::default();
    params.insert("gain", ParameterValue::Float(1.0));
    let src = AudioBlock {
        id: BlockId("g".into()),
        enabled: true,
        kind: AudioBlockKind::Core(CoreBlock {
            effect_type: "gain".into(),
            model: "volume".into(),
            params,
        }),
    };
    let mut r = rig();
    r.inputs.get_mut("input-1").unwrap().active_preset = 1; // clean
    r.presets.get_mut("clean").unwrap().blocks = vec![src.clone()];

    let slot = r.add_preset_to_input("input-1").expect("added");
    let new_name = r.inputs["input-1"].bank[&slot].clone();
    // New preset is empty by design. Push a block manually so we can
    // edit it via the synthetic chain.
    r.presets.get_mut(&new_name).unwrap().blocks = vec![src.clone()];

    let mut proj = rig_to_legacy_project(&r, &BTreeSet::new());
    for c in proj.chains.iter_mut().filter(|c| c.id.0 == "rig:input-1") {
        for b in c.blocks.iter_mut() {
            if let AudioBlockKind::Core(core) = &mut b.kind {
                core.params.insert("gain", ParameterValue::Float(9.0));
            }
        }
    }
    super::sync_synthetic_into_rig(&mut r, &proj);

    let read_gain = |p: &project::rig::RigPreset, scene: usize| match &p.apply_scene(scene)[0].kind
    {
        AudioBlockKind::Core(c) => c.params.get_f32("gain"),
        _ => None,
    };
    assert_eq!(
        read_gain(&r.presets[&new_name], r.inputs["input-1"].active_scene),
        Some(9.0),
        "edit landed in the NEW preset"
    );
    assert_eq!(
        read_gain(&r.presets["clean"], 1),
        Some(1.0),
        "source preset 'clean' is untouched"
    );
}

#[test]
fn nav_row_exposes_scene_count_and_grows_when_a_scene_is_added() {
    let mut r = rig();
    let rows = rig_nav_rows(&r, &rig_to_legacy_project(&r, &BTreeSet::new()));
    assert_eq!(rows[0].scene_count, 1, "starts with a single scene");

    r.add_scene_to_input("input-1").expect("scene added");
    let rows = rig_nav_rows(&r, &rig_to_legacy_project(&r, &BTreeSet::new()));
    assert_eq!(rows[0].scene_count, 2, "now two scenes");
    assert_eq!(rows[0].scene, 2, "the added scene is active");
}

// End-to-end through the REAL engine path the GUI uses: a per-scene
// volume override must surface as the projected chain's volume, and
// switching scenes must switch the volume — this is exactly the
// "salvei 100% na scene 2, agora todas as scenes com 100%" bug.
#[test]
fn switching_scene_projects_that_scenes_volume() {
    let mut r = rig(); // active_scene 4
    r.presets.get_mut("drive").unwrap().volume = 80.0; // active preset
    r.inputs.get_mut("input-1").unwrap().active_scene = 1;

    // scene 1 inherits the preset volume.
    let c1 = switch_and_project_input(&mut r, "input-1", None, Some(1)).expect("s1");
    assert_eq!(c1.volume, 80.0, "scene 1 = preset volume");

    // Add scene 2 and set its volume to 100 via the GUI write-back path.
    let s2 = r.add_scene_to_input("input-1").expect("scene 2");
    r.write_back_chain_volume("input-1", 100.0);

    let c2 = switch_and_project_input(&mut r, "input-1", None, Some(s2)).expect("s2");
    assert_eq!(c2.volume, 100.0, "scene 2 = its own 100");
    let back1 = switch_and_project_input(&mut r, "input-1", None, Some(1)).expect("s1");
    assert_eq!(
        back1.volume, 80.0,
        "scene 1 still 80 — per-scene, not bled to all"
    );
}

// Issue #535: the SceneBar reads `scene_count` per chain row from the
// ACTIVE preset only. Adding a scene to A must not change the count the
// row exposes once the user switches the combobox to a sibling preset B.
#[test]
fn nav_row_scene_count_follows_active_preset_after_a_sibling_grew() {
    // Bank {1:clean, 2:drive, 3:lead}; start active=clean (slot 1), scene 1.
    let mut r = rig();
    r.inputs.get_mut("input-1").unwrap().active_preset = 1;
    r.inputs.get_mut("input-1").unwrap().active_scene = 1;

    // 1. + scene on "clean" (active). Row must now read 2.
    r.add_scene_to_input("input-1").expect("scene added on clean");
    let rows = rig_nav_rows(&r, &rig_to_legacy_project(&r, &BTreeSet::new()));
    assert_eq!(rows[0].scene_count, 2, "active preset 'clean' has 2 scenes");

    // 2. Combobox switch to "drive" (slot 2 — never touched).
    switch_and_project_input(&mut r, "input-1", Some(2), None).expect("switched");

    // 3. Row's scene_count MUST reflect drive's pool entry (= 1), NOT a
    //    stale 2 from clean nor a leaked sibling.
    let rows = rig_nav_rows(&r, &rig_to_legacy_project(&r, &BTreeSet::new()));
    assert_eq!(
        rows[0].scene_count, 1,
        "after switching to a sibling preset that never had a scene added, the row must show 1 scene"
    );
}

#[test]
fn non_rig_chain_yields_empty_row() {
    let r = rig();
    let mut proj = rig_to_legacy_project(&r, &BTreeSet::new());
    proj.chains[0].id = domain::ids::ChainId("legacy-thing".into());
    let rows = rig_nav_rows(&r, &proj);
    assert_eq!(rows[0], RigNavRow::default(), "no selectors for non-rig");
}

#[test]
fn sync_synthetic_into_rig_writes_edited_blocks_back_to_active_preset() {
    use domain::ids::BlockId;
    use domain::value_objects::ParameterValue;
    use project::block::{AudioBlock, AudioBlockKind, CoreBlock};
    use project::param::ParameterSet;

    let mut params = ParameterSet::default();
    params.insert("volume", ParameterValue::Float(80.0));
    let blk = AudioBlock {
        id: BlockId("vol".into()),
        enabled: true,
        kind: AudioBlockKind::Core(CoreBlock {
            effect_type: "gain".into(),
            model: "volume".into(),
            params,
        }),
    };
    let mut r = rig();
    r.presets.get_mut("clean").unwrap().blocks = vec![blk.clone()];
    // active preset of input-1 is slot 2 ("drive") in rig(); point it at clean.
    r.inputs.get_mut("input-1").unwrap().active_preset = 1;

    let mut proj = rig_to_legacy_project(&r, &BTreeSet::new());
    // User edits the param on the projected synthetic chain.
    for c in proj.chains.iter_mut().filter(|c| c.id.0 == "rig:input-1") {
        for b in c.blocks.iter_mut() {
            if let AudioBlockKind::Core(core) = &mut b.kind {
                core.params.insert("volume", ParameterValue::Float(90.0));
            }
        }
    }

    let sc = r.inputs["input-1"].active_scene;
    super::sync_synthetic_into_rig(&mut r, &proj);

    // Snapshot: the edit lands in the active scene, not the template.
    let preset = r.presets.get("clean").unwrap();
    let active = match &preset.apply_scene(sc)[0].kind {
        AudioBlockKind::Core(c) => c.params.get_f32("volume"),
        _ => None,
    };
    assert_eq!(
        active,
        Some(90.0),
        "synthetic edit written into active scene"
    );
    let base = match &preset.blocks[0].kind {
        AudioBlockKind::Core(c) => c.params.get_f32("volume"),
        _ => None,
    };
    assert_eq!(base, Some(80.0), "factory template untouched");
}

// User repro (#436): the chain TITLE (the big text in the title slot)
// shows the slug/input id, same bug as the preset select. The title is
// chains[i].description from rig_to_legacy_project. It must show the
// active preset's human name, not the input id.
#[test]
fn chain_title_shows_active_preset_name_not_the_input_id() {
    let mut r = rig(); // input-1, presets clean/drive/lead, active preset 2
    let active = r.inputs["input-1"].active_preset;
    let key = r.inputs["input-1"].bank[&active].clone();
    r.presets.get_mut(&key).unwrap().name = Some("SILVERCHAIR FREAK - SCARLETT".into());

    let proj = rig_to_legacy_project(&r, &BTreeSet::new());
    assert_eq!(
        proj.chains[0].description.as_deref(),
        Some("SILVERCHAIR FREAK - SCARLETT"),
        "title must be the active preset's name, not the input id"
    );
}

// User repro (#436): "mesma merda" — fix só ajudou migração nova. Um
// projeto JÁ SALVO tem RigPreset com name: None (serde default, salvo
// antes do campo). Ao reabrir, select/título devem mostrar algo
// legível, NÃO o slug cru "studio-clean-compressor".
#[test]
fn legacy_preset_without_name_shows_humanized_label_not_raw_slug() {
    let mut r = rig();
    let active = r.inputs["input-1"].active_preset;
    let key = r.inputs["input-1"].bank[&active].clone(); // "drive"
                                                         // Simula um preset de projeto antigo: id = slug, SEM name.
    {
        let p = r.presets.remove(&key).unwrap();
        r.presets.insert(
            "studio-clean-compressor".to_string(),
            RigPreset {
                id: "studio-clean-compressor".to_string(),
                name: None,
                ..p
            },
        );
    }
    r.inputs
        .get_mut("input-1")
        .unwrap()
        .bank
        .insert(active, "studio-clean-compressor".to_string());

    let rows = rig_nav_rows(&r, &rig_to_legacy_project(&r, &BTreeSet::new()));
    let label = &rows[0].preset_labels[rows[0].active_index];
    assert_eq!(
        label, "Studio Clean Compressor",
        "preset antigo sem name deve exibir o id humanizado, não o slug cru"
    );
}
