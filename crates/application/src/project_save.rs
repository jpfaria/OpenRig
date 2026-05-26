//! Pure helpers used by `Command::SaveProject` to turn the dispatcher's
//! in-memory project + rig into the on-disk artifacts (#555 round 2).
//!
//! Lifted out of `adapter-gui::project_ops` so the dispatcher (and any
//! future MCP / gRPC adapter) can call them without depending on Slint
//! or the GUI's `ProjectSession` struct.

use std::collections::BTreeSet;

use anyhow::Result;

use project::chain::Chain;
use project::project::Project;
use project::rig::{RigPreset, RigProject, RigScene};

/// Snapshot the project as a string the GUI compares against to decide
/// whether the user has unsaved edits. The rig is part of the dirty
/// fingerprint because switching preset/scene/source often produces an
/// identical legacy `Project` even though the rig changed — without
/// including the rig, the user could lose work via "no changes to save".
///
/// Pure: `(Project, Option<&RigProject>) -> Result<String>`.
pub fn dirty_snapshot(project: &Project, rig: Option<&RigProject>) -> Result<String> {
    let legacy = infra_yaml::serialize_project(project)?;
    match rig {
        Some(rig) => Ok(format!(
            "{legacy}\n---openrig---\n{}",
            infra_yaml::serialize_rig_project(rig)?
        )),
        None => Ok(legacy),
    }
}

/// Build the `RigProject` that will actually hit disk on save: starts
/// from the in-memory rig (or an empty one for legacy projects),
/// migrates any newly-created legacy chains into their own input +
/// preset bank, drops projected chains the user removed, and garbage-
/// collects orphan presets / outputs.
///
/// Pure: `(&Project, Option<&RigProject>) -> RigProject`. Mirrors the
/// previous `adapter-gui::project_ops::build_rig_for_save` 1:1; the
/// only change is the removal of the `session.dispatcher.dispatch(
/// CaptureRigEdits)` self-call — callers MUST dispatch
/// `Command::CaptureRigEdits` themselves *before* invoking this if
/// they want pending in-flight rig edits flushed first.
pub fn build_rig_for_save(project: &Project, current_rig: Option<&RigProject>) -> RigProject {
    let mut rig_out = match current_rig {
        Some(rig) => rig.clone(),
        None => RigProject {
            name: project.name.clone(),
            inputs: std::collections::BTreeMap::new(),
            outputs: std::collections::BTreeMap::new(),
            presets: std::collections::BTreeMap::new(),
            midi: None,
            chain_order: Vec::new(),
        },
    };
    // 1. New chains (not projected from the rig) → each becomes its
    //    own input + preset bank. Migrating per-chain (rather than the
    //    whole legacy project at once) avoids `migrate_legacy_project`'s
    //    auto-grouping by capture source: two chains that happen to
    //    share a device must remain two independent inputs because the
    //    user explicitly created two chains.
    let new_chains: Vec<Chain> = project
        .chains
        .iter()
        .filter(|c| !c.id.0.starts_with("rig:"))
        .cloned()
        .collect();
    let mut newly_added_inputs: BTreeSet<String> = BTreeSet::new();
    for chain in new_chains {
        let temp = Project {
            name: None,
            device_settings: Vec::new(),
            chains: vec![chain],
            midi: None,
        };
        let mut migrated = project::migrate::migrate_legacy_project(&temp);
        // Single-chain migration ⇒ exactly one input ("input-1"). Pop
        // it, retarget the bank entry to a unique preset key, set the
        // visible "Preset 1" default, and ensure scene 1 exists.
        let (_old_input_name, mut input) = migrated
            .inputs
            .iter()
            .next()
            .map(|(k, v)| (k.clone(), v.clone()))
            .expect("single-chain migration produces exactly one input");
        // Generate the next unique input slot in `rig_out`.
        let next_n = rig_out
            .inputs
            .keys()
            .chain(newly_added_inputs.iter())
            .filter_map(|k| {
                k.strip_prefix("input-")
                    .and_then(|n| n.parse::<usize>().ok())
            })
            .max()
            .unwrap_or(0)
            + 1;
        let new_input_name = format!("input-{next_n}");
        // The migrated bank's slot 1 names a preset (slug of the chain
        // description). Two chains can slug to the same key, so we
        // ensure uniqueness in `rig_out.presets`.
        let old_preset_key = input
            .bank
            .get(&1)
            .cloned()
            .expect("migrated input bank slot 1 exists");
        let mut preset: RigPreset = migrated
            .presets
            .remove(&old_preset_key)
            .expect("preset for bank slot 1 exists");
        let mut final_preset_key = old_preset_key.clone();
        let mut suffix = 2;
        while rig_out.presets.contains_key(&final_preset_key) {
            final_preset_key = format!("{old_preset_key}-{suffix}");
            suffix += 1;
        }
        if final_preset_key != old_preset_key {
            input.bank.insert(1, final_preset_key.clone());
        }
        preset.id = final_preset_key.clone();
        // Distinct, user-facing preset label so the chain name and the
        // preset name don't collide ("Chain 1" vs "Preset 1").
        preset.name = Some("Preset 1".to_string());
        // Make scene 1 an addressable slot so the user can edit it
        // without a "create scene" step.
        preset.scenes.entry(1).or_insert_with(RigScene::default);

        rig_out.inputs.insert(new_input_name.clone(), input);
        rig_out.presets.insert(final_preset_key, preset);
        newly_added_inputs.insert(new_input_name);

        for (name, output) in migrated.outputs {
            rig_out.outputs.entry(name).or_insert(output);
        }
    }
    // 2. Projected chains the user removed → drop the matching inputs
    //    so the reload doesn't resurrect them.
    let surviving_projected: BTreeSet<String> = project
        .chains
        .iter()
        .filter_map(|c| c.id.0.strip_prefix("rig:").map(String::from))
        .collect();
    rig_out
        .inputs
        .retain(|name, _| surviving_projected.contains(name) || newly_added_inputs.contains(name));
    // Garbage-collect orphan presets / outputs no longer referenced.
    let referenced_presets: BTreeSet<String> = rig_out
        .inputs
        .values()
        .flat_map(|i| i.bank.values().cloned())
        .collect();
    rig_out
        .presets
        .retain(|name, _| referenced_presets.contains(name));
    let referenced_outputs: BTreeSet<String> = rig_out
        .inputs
        .values()
        .flat_map(|i| i.routing.iter().cloned())
        .collect();
    rig_out
        .outputs
        .retain(|name, _| referenced_outputs.contains(name));
    // Carry the user-visible project name (`UpdateProjectName` writes
    // to the legacy `Project`; the rig must mirror it on disk).
    rig_out.name = project.name.clone();
    rig_out
}
