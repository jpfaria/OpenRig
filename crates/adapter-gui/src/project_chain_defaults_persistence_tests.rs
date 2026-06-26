//! Red-first tests for the chain-creation defaults the user wants:
//!
//! 1. A newly created chain saves with `input.label` set to the chain
//!    description, but `preset.name` is a distinct default (e.g.
//!    "Preset 1") — never a duplicate of the chain name.
//! 2. The first scene exists explicitly in `preset.scenes` so the
//!    user can edit scene 1 without having to "create" it first.
//! 3. Two chains that share the same capture source do NOT collapse
//!    into one rig input — each chain is its own input with its own
//!    preset bank. (The legacy auto-grouping is the source of the
//!    "input config is of the preset" confusion.)
//! 4. A freshly created project auto-creates a "default" I/O binding
//!    from the system default devices and the new chain head/tail blocks
//!    reference it (#716, Task 20, O4).
//!
//! All tests exercise the same `create_new_project_session` →
//! `save_project_session` → `load_rig_project_file` path the GUI
//! follows. We read the on-disk `.openrig` directly so the assertions
//! are about what was persisted, not about the in-memory rig.

use crate::default_io_binding::DEFAULT_BINDING_ID;
use crate::project_ops::{create_new_project_session, save_project_session};
use crate::state::ProjectSession;
use application::chain_factory::{build_default_chain, DefaultChainParams, EndpointSpec};
use infra_filesystem::AppConfig;
use project::block::{AudioBlockKind, InputBlock, OutputBlock};
use project::chain::Chain;
use std::path::PathBuf;
use tempfile::TempDir;

struct Sandbox {
    _tmp: TempDir,
    path: PathBuf,
    cfg: PathBuf,
}

impl Sandbox {
    fn new() -> Self {
        let tmp = TempDir::new().unwrap();
        let path = tmp.path().join("project.yaml");
        let cfg = tmp.path().join("config.yaml");
        Self {
            _tmp: tmp,
            path,
            cfg,
        }
    }

    fn new_session(&self) -> ProjectSession {
        let mut s = create_new_project_session(&self.cfg);
        s.project_path = Some(self.path.clone());
        s.config_path = Some(self.cfg.clone());
        s
    }

    fn save(&self, session: &ProjectSession) {
        save_project_session(session, &self.path).expect("save");
    }

    fn read_openrig(&self) -> project::rig::RigProject {
        // #716: the project persists as the `.yaml` itself now (no `.openrig`).
        infra_yaml::load_rig_project_file(&self.path).expect("load saved rig (.yaml)")
    }
}

fn chain_with(session: &ProjectSession, desc: &str, dev: &str) -> Chain {
    build_default_chain(DefaultChainParams {
        project: &session.project.borrow(),
        instrument: "electric_guitar",
        description: Some(desc.into()),
        input: EndpointSpec {
            device_id: Some(dev),
            channels: vec![0],
            io: String::new(),
            endpoint: String::new(),
        },
        output: EndpointSpec {
            device_id: Some("test-out"),
            channels: vec![0, 1],
            io: String::new(),
            endpoint: String::new(),
        },
    })
}

// ────────────────────────────────────────────────────────────────────
// 1. Chain name vs preset name
// ────────────────────────────────────────────────────────────────────

#[test]
fn new_chain_save_sets_input_label_to_chain_description() {
    let s = Sandbox::new();
    let session = s.new_session();
    let c = chain_with(&session, "Chain 1", "dev-A");
    session.project.borrow_mut().chains.push(c);
    s.save(&session);

    let rig = s.read_openrig();
    let labels: Vec<Option<String>> = rig.inputs.values().map(|i| i.label.clone()).collect();
    assert!(
        labels.contains(&Some("Chain 1".to_string())),
        "input.label must carry the chain description; got {labels:?}"
    );
}

#[test]
fn new_chain_save_does_not_duplicate_chain_name_into_preset_name() {
    let s = Sandbox::new();
    let session = s.new_session();
    let c = chain_with(&session, "Chain 1", "dev-A");
    session.project.borrow_mut().chains.push(c);
    s.save(&session);

    let rig = s.read_openrig();
    let preset_names: Vec<Option<String>> = rig.presets.values().map(|p| p.name.clone()).collect();
    assert!(
        !preset_names.iter().any(|n| n.as_deref() == Some("Chain 1")),
        "preset.name must be distinct from the chain name; got {preset_names:?}"
    );
}

#[test]
fn new_chain_save_uses_default_preset_label() {
    // The preset should ship with a sensible default name so the user
    // can see something in the preset combobox; "Preset 1" is the
    // first slot of a brand-new chain's bank.
    let s = Sandbox::new();
    let session = s.new_session();
    let c = chain_with(&session, "Chain 1", "dev-A");
    session.project.borrow_mut().chains.push(c);
    s.save(&session);

    let rig = s.read_openrig();
    let preset_names: Vec<Option<String>> = rig.presets.values().map(|p| p.name.clone()).collect();
    assert!(
        preset_names
            .iter()
            .any(|n| n.as_deref() == Some("Preset 1")),
        "expected a 'Preset 1' default; got {preset_names:?}"
    );
}

// ────────────────────────────────────────────────────────────────────
// 2. Scene 1 must exist explicitly so it can be edited
// ────────────────────────────────────────────────────────────────────

#[test]
fn new_chain_save_creates_scene_1_explicitly() {
    let s = Sandbox::new();
    let session = s.new_session();
    let c = chain_with(&session, "Chain 1", "dev-A");
    session.project.borrow_mut().chains.push(c);
    s.save(&session);

    let rig = s.read_openrig();
    // For every preset on the rig, scene 1 must exist as an entry of
    // `preset.scenes` (an empty `RigScene` is fine — the point is the
    // slot is editable, not requiring a "Create scene" action).
    for (preset_name, preset) in &rig.presets {
        assert!(
            preset.scenes.contains_key(&1),
            "preset {preset_name:?} is missing scene 1 (scenes: {:?})",
            preset.scenes.keys().collect::<Vec<_>>()
        );
    }
}

// ────────────────────────────────────────────────────────────────────
// 3. Two chains sharing the same source must NOT collapse into one input
// ────────────────────────────────────────────────────────────────────

#[test]
fn two_chains_with_same_source_save_as_two_separate_inputs() {
    let s = Sandbox::new();
    let session = s.new_session();
    // Both chains tap the same Scarlett channel — under the old
    // grouping behaviour this collapses into ONE input with TWO
    // presets. The fix is: each chain is its own input.
    let a = chain_with(&session, "Chain 1", "shared-dev");
    let b = chain_with(&session, "Chain 2", "shared-dev");
    session.project.borrow_mut().chains.push(a);
    session.project.borrow_mut().chains.push(b);
    s.save(&session);

    let rig = s.read_openrig();
    assert_eq!(
        rig.inputs.len(),
        2,
        "two distinct chains must produce two distinct inputs, not be \
         grouped (got {} input(s): {:?})",
        rig.inputs.len(),
        rig.inputs.keys().collect::<Vec<_>>()
    );
    let labels: Vec<Option<String>> = rig.inputs.values().map(|i| i.label.clone()).collect();
    assert!(
        labels.contains(&Some("Chain 1".to_string()))
            && labels.contains(&Some("Chain 2".to_string())),
        "both chain labels must survive into separate inputs; got {labels:?}"
    );
}

#[test]
fn two_chains_with_same_source_save_as_two_independent_preset_pools() {
    // Stronger variant: each chain must own its own preset bank,
    // not share one. Two inputs ⇒ two banks ⇒ two presets total.
    let s = Sandbox::new();
    let session = s.new_session();
    let a = chain_with(&session, "Chain 1", "shared-dev");
    let b = chain_with(&session, "Chain 2", "shared-dev");
    session.project.borrow_mut().chains.push(a);
    session.project.borrow_mut().chains.push(b);
    s.save(&session);

    let rig = s.read_openrig();
    let total_bank_entries: usize = rig.inputs.values().map(|i| i.bank.len()).sum();
    assert_eq!(
        total_bank_entries,
        2,
        "each chain owns its own bank (expected 2 total slots, got {total_bank_entries}; \
         inputs: {:?})",
        rig.inputs
            .iter()
            .map(|(k, i)| (k.clone(), i.bank.clone()))
            .collect::<Vec<_>>()
    );
}

// ────────────────────────────────────────────────────────────────────
// 4. Consistency check across all three defaults
// ────────────────────────────────────────────────────────────────────

#[test]
fn new_chain_defaults_are_consistent_end_to_end() {
    let s = Sandbox::new();
    let session = s.new_session();
    let c = chain_with(&session, "GUITAR", "dev-X");
    session.project.borrow_mut().chains.push(c);
    s.save(&session);

    let rig = s.read_openrig();
    assert_eq!(rig.inputs.len(), 1, "one chain → one input");
    let (input_name, input) = rig.inputs.iter().next().unwrap();
    assert_eq!(
        input.label.as_deref(),
        Some("GUITAR"),
        "input.label is chain name"
    );
    assert_eq!(input.bank.len(), 1, "exactly one preset slot");
    let preset_key = input.bank.get(&1).expect("bank slot 1 present");
    let preset = rig.presets.get(preset_key).expect("preset exists");
    assert_eq!(
        preset.name.as_deref(),
        Some("Preset 1"),
        "preset.name is the default 'Preset 1', not the chain name (input {input_name:?})"
    );
    assert!(
        preset.scenes.contains_key(&1),
        "scene 1 is present on the new preset"
    );
}

// ────────────────────────────────────────────────────────────────────
// 4. Default I/O binding auto-created for fresh projects (#716, Task 20)
// ────────────────────────────────────────────────────────────────────

/// Write an AppConfig with one input and one output device to a temp
/// config.yaml so `create_new_project_session` can read the system
/// defaults without touching the real OS config path (#701).
fn write_config_with_devices(cfg_path: &PathBuf, input_id: &str, output_id: &str) {
    let config = AppConfig {
        input_devices: vec![infra_filesystem::GuiAudioDeviceSettings {
            device_id: input_id.to_string(),
            name: "Test Input".to_string(),
            sample_rate: 44100,
            buffer_size_frames: 256,
            bit_depth: 24,
        }],
        output_devices: vec![infra_filesystem::GuiAudioDeviceSettings {
            device_id: output_id.to_string(),
            name: "Test Output".to_string(),
            sample_rate: 44100,
            buffer_size_frames: 256,
            bit_depth: 24,
        }],
        ..AppConfig::default()
    };
    let raw = serde_yaml::to_string(&config).expect("serialize AppConfig");
    std::fs::write(cfg_path, raw).expect("write config.yaml");
}

/// Load an AppConfig from a path (mirrors the private helper in
/// `local_dispatcher_io_binding` so the test stays self-contained).
fn read_app_config(cfg_path: &PathBuf) -> AppConfig {
    let raw = std::fs::read_to_string(cfg_path).expect("read config.yaml");
    serde_yaml::from_str(&raw).expect("parse AppConfig")
}

#[test]
fn new_project_has_default_binding() {
    // RED: `create_new_project_session` must auto-create a "default" binding
    // in AppConfig when the config already carries device settings.
    let tmp = TempDir::new().unwrap();
    let cfg = tmp.path().join("config.yaml");
    let path = tmp.path().join("project.yaml");

    let input_id = "test-input-dev";
    let output_id = "test-output-dev";
    write_config_with_devices(&cfg, input_id, output_id);

    // Create session — this must auto-create the "default" binding.
    let mut session = create_new_project_session(&cfg);
    session.project_path = Some(path.clone());
    session.config_path = Some(cfg.clone());

    // Verify binding exists in AppConfig on disk.
    let config = read_app_config(&cfg);
    let binding = config
        .io_bindings
        .iter()
        .find(|b| b.id == DEFAULT_BINDING_ID);
    assert!(
        binding.is_some(),
        "expected a '{DEFAULT_BINDING_ID}' binding in AppConfig after new-project creation; \
         got io_bindings: {:?}",
        config.io_bindings
    );
    let binding = binding.unwrap();

    // The binding must reference the actual device IDs from config.
    let first_input_ep = binding
        .inputs
        .first()
        .expect("binding must have at least one input endpoint");
    let first_output_ep = binding
        .outputs
        .first()
        .expect("binding must have at least one output endpoint");
    assert_eq!(
        first_input_ep.device_id.0, input_id,
        "default binding input device must match config"
    );
    assert_eq!(
        first_output_ep.device_id.0, output_id,
        "default binding output device must match config"
    );

    // Verify that a chain added to the project can reference the binding —
    // the standard factory supports the binding reference via io/endpoint.
    let c = build_default_chain(DefaultChainParams {
        project: &session.project.borrow(),
        instrument: "electric_guitar",
        description: Some("Guitar".into()),
        input: EndpointSpec {
            device_id: None,
            channels: vec![0],
            io: DEFAULT_BINDING_ID.to_string(),
            endpoint: first_input_ep.name.clone(),
        },
        output: EndpointSpec {
            device_id: None,
            channels: vec![0, 1],
            io: DEFAULT_BINDING_ID.to_string(),
            endpoint: first_output_ep.name.clone(),
        },
    });
    // Head Input block must carry io="default" and the input endpoint name.
    let head = c.blocks.first().expect("chain has at least one block");
    let AudioBlockKind::Input(InputBlock { io, endpoint, .. }) = &head.kind else {
        panic!("first block must be an Input block");
    };
    assert_eq!(io, DEFAULT_BINDING_ID, "head block io must be 'default'");
    assert_eq!(
        endpoint,
        &first_input_ep.name,
        "head block endpoint must match binding input endpoint name"
    );

    // Tail Output block must carry io="default" and the output endpoint name.
    let tail = c.blocks.last().expect("chain has at least one block");
    let AudioBlockKind::Output(OutputBlock {
        io: out_io,
        endpoint: out_endpoint,
        ..
    }) = &tail.kind
    else {
        panic!("last block must be an Output block");
    };
    assert_eq!(out_io, DEFAULT_BINDING_ID, "tail block io must be 'default'");
    assert_eq!(
        out_endpoint,
        &first_output_ep.name,
        "tail block endpoint must match binding output endpoint name"
    );
}

#[test]
fn new_project_reuses_existing_default() {
    // RED: if a "default" binding already exists in config, creating a new
    // project session must NOT add a duplicate — the binding count stays at 1.
    let tmp = TempDir::new().unwrap();
    let cfg = tmp.path().join("config.yaml");
    let path = tmp.path().join("project.yaml");

    let input_id = "test-input-dev";
    let output_id = "test-output-dev";
    write_config_with_devices(&cfg, input_id, output_id);

    // First session creation — inserts the binding.
    let mut s1 = create_new_project_session(&cfg);
    s1.project_path = Some(path.clone());
    s1.config_path = Some(cfg.clone());

    // Second session creation — must reuse the existing binding.
    let mut s2 = create_new_project_session(&cfg);
    s2.project_path = Some(path.clone());
    s2.config_path = Some(cfg.clone());

    let config = read_app_config(&cfg);
    let default_count = config
        .io_bindings
        .iter()
        .filter(|b| b.id == DEFAULT_BINDING_ID)
        .count();
    assert_eq!(
        default_count, 1,
        "exactly one '{DEFAULT_BINDING_ID}' binding expected; got {default_count} \
         (io_bindings: {:?})",
        config.io_bindings
    );
}
