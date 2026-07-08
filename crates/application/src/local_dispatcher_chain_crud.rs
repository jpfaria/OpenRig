//! Chain CRUD handler (file-per-feature; #436 dispatcher split).
//! Behaviour byte-identical to the original inline arm — pure move.

use anyhow::Result;

use crate::command::Command;
use crate::event::Event;
use crate::local_dispatcher::LocalDispatcher;

impl LocalDispatcher {
    /// Chain CRUD commands: add/configure/remove/volume.
    pub(crate) fn handle_chain_crud(&self, cmd: Command) -> Result<Vec<Event>> {
        match cmd {
            // ── Chain CRUD ────────────────────────────────────────────────────
            Command::AddChain { mut chain } => {
                // #716 (model A): the per-block cross-chain channel-conflict
                // check is gone — device endpoints no longer live on the chain,
                // they are resolved from the per-machine binding registry at
                // activation time, where the conflict check now belongs.
                // Mirror the new chain into the attached rig (if any),
                // and re-tag the chain's id to `rig:<input>` so the
                // chains-screen rig nav can locate it. Without the
                // `rig:` prefix `rig_nav_rows` falls back to an empty
                // `RigNavRow`, leaving the preset combobox blank.
                if let Some(rig) = self.rig.borrow().clone() {
                    if let Some(input_name) = add_chain_to_rig(&mut rig.borrow_mut(), &chain) {
                        chain.id = domain::ids::ChainId(format!("rig:{input_name}"));
                    }
                }
                let chain_id = chain.id.clone();
                self.project.borrow_mut().chains.push(chain);
                Ok(vec![
                    Event::ChainAdded { chain: chain_id },
                    Event::ProjectMutated,
                ])
            }
            Command::ConfigureChain { chain } => {
                let chain_id = chain.id.clone();
                self.with_chain(&chain_id, |existing| {
                    // Preserve runtime-only state (enabled) — callers must use
                    // ToggleChainEnabled to change the running state.
                    let keep_enabled = existing.enabled;
                    *existing = chain;
                    existing.enabled = keep_enabled;
                    Ok(())
                })?;
                Ok(vec![
                    Event::ChainConfigured { chain: chain_id },
                    Event::ProjectMutated,
                ])
            }
            Command::RemoveChain { chain } => {
                {
                    let mut proj = self.project.borrow_mut();
                    let pre_len = proj.chains.len();
                    proj.chains.retain(|c| c.id != chain);
                    if proj.chains.len() == pre_len {
                        return Err(anyhow::anyhow!("chain not found: {:?}", chain));
                    }
                }
                // #436: a rig chain (`rig:<input>`) must also drop its
                // RigInput, else any re-projection resurrects it. This
                // used to be done by hand in the GUI — now it's here.
                if let (Some(rig), Some(name)) =
                    (self.rig.borrow().clone(), chain.0.strip_prefix("rig:"))
                {
                    rig.borrow_mut().remove_input(name);
                }
                Ok(vec![Event::ChainRemoved { chain }, Event::ProjectMutated])
            }
            // ── Chain volume (issue #440) ─────────────────────────────────────
            Command::SetChainVolume { chain, value } => {
                self.with_chain(&chain, |c| {
                    c.volume = value;
                    Ok(())
                })?;
                Ok(vec![
                    Event::ChainVolumeChanged { chain, value },
                    Event::ProjectMutated,
                ])
            }
            // ── Chain I/O binding selection (issue #716) ──────────────────────
            Command::SetChainIoBindings { chain, binding_ids } => {
                self.with_chain(&chain, |c| {
                    c.io_binding_ids = binding_ids.clone();
                    Ok(())
                })?;
                // #716: propagate the selection into the rig so it survives
                // reopen (the rig is the persistence model; rig→legacy
                // reprojects from it). Without this the checklist reopens
                // unchecked and the chain reopens unbound (no runtime).
                if let Some(input_name) = chain.0.strip_prefix("rig:") {
                    if let Some(rig) = self.rig.borrow().clone() {
                        if let Some(rig_input) = rig.borrow_mut().inputs.get_mut(input_name) {
                            rig_input.io_binding_ids = binding_ids.clone();
                        }
                    }
                }
                Ok(vec![
                    Event::ChainIoBindingsChanged { chain, binding_ids },
                    Event::ProjectMutated,
                ])
            }
            other => unreachable!("handle_chain_crud received non-crud command: {other:?}"),
        }
    }
}

/// Add a single legacy [`Chain`] into the given [`RigProject`] as its
/// own input + preset (no auto-grouping by source). The preset gets
/// the visible default name `"Preset 1"` and an explicit scene 1 slot
/// so the GUI's combobox is populated immediately after `AddChain`
/// runs — no save/reload required.
pub(crate) fn add_chain_to_rig(
    rig: &mut project::rig::RigProject,
    chain: &project::chain::Chain,
) -> Option<String> {
    let temp = project::project::Project {
        name: None,
        device_settings: Vec::new(),
        chains: vec![chain.clone()],
        midi: None,
    };
    let mut migrated = project::migrate::migrate_legacy_project(&temp);

    let (_old_name, mut input) = migrated
        .inputs
        .iter()
        .next()
        .map(|(k, v)| (k.clone(), v.clone()))?;

    let next_n = rig
        .inputs
        .keys()
        .filter_map(|k| {
            k.strip_prefix("input-")
                .and_then(|n| n.parse::<usize>().ok())
        })
        .max()
        .unwrap_or(0)
        + 1;
    let new_input_name = format!("input-{next_n}");

    let old_preset_key = input.bank.get(&1).cloned()?;
    let mut preset = migrated.presets.remove(&old_preset_key)?;

    // Ensure a unique preset key inside `rig.presets`.
    let mut final_preset_key = old_preset_key.clone();
    let mut suffix = 2;
    while rig.presets.contains_key(&final_preset_key) {
        final_preset_key = format!("{old_preset_key}-{suffix}");
        suffix += 1;
    }
    if final_preset_key != old_preset_key {
        input.bank.insert(1, final_preset_key.clone());
    }
    preset.id = final_preset_key.clone();
    preset.name = Some("Preset 1".to_string());
    preset
        .scenes
        .entry(1)
        .or_insert_with(project::rig::RigScene::default);

    rig.inputs.insert(new_input_name.clone(), input);
    rig.presets.insert(final_preset_key, preset);

    for (name, output) in migrated.outputs {
        rig.outputs.entry(name).or_insert(output);
    }
    Some(new_input_name)
}
