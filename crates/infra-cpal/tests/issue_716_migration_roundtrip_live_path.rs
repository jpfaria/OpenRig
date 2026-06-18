//! Issue #716 (defect C2) — migration roundtrip through the LIVE build path.
//!
//! `migrate_legacy_io` rewrites a legacy chain's Input/Output blocks from
//! `entries` to `{ io, endpoint }` and DRAINS `entries`. The live engine read
//! ONLY `entries`, so after migration the same chain built through the live
//! seam (`build_chain_runtime`) produced no real audio (`resolve` bailed /
//! the fallback runtime carried silence) — migrated projects went SILENT.
//!
//! This test builds the legacy chain through the live path (empty registry),
//! then migrates it and builds the migrated chain through the live path with
//! the registry the migration produced, and asserts the rendered audio is
//! equivalent. RED on the dead-code state: the migrated (drained-entries)
//! build carries no energy because the live path ignores `io_bindings`.

use std::sync::Arc;

use domain::ids::{BlockId, ChainId, DeviceId};
use domain::io_binding::IoBinding;
use engine::runtime::{process_input_f32, process_output_f32, ChainRuntimeState};
use infra_cpal::{build_chain_runtime, BuildRequest};
use project::block::{
    AudioBlock, AudioBlockKind, InputBlock, InputEntry, OutputBlock, OutputEntry,
};
use project::chain::{Chain, ChainInputMode, ChainOutputMode};
use project::project::Project;

fn legacy_input(id: &str, device: &str) -> AudioBlock {
    AudioBlock {
        id: BlockId(id.into()),
        enabled: true,
        kind: AudioBlockKind::Input(InputBlock {
            model: "standard".into(),
            io: String::new(),
            endpoint: String::new(),
            entries: vec![InputEntry {
                device_id: DeviceId(device.into()),
                mode: ChainInputMode::Mono,
                channels: vec![0],
            }],
        }),
    }
}

fn legacy_output(id: &str, device: &str) -> AudioBlock {
    AudioBlock {
        id: BlockId(id.into()),
        enabled: true,
        kind: AudioBlockKind::Output(OutputBlock {
            model: "standard".into(),
            io: String::new(),
            endpoint: String::new(),
            entries: vec![OutputEntry {
                device_id: DeviceId(device.into()),
                mode: ChainOutputMode::Mono,
                channels: vec![0],
            }],
        }),
    }
}

/// Single-binding passthrough: input(dev_x) → output(dev_x).
fn legacy_chain() -> Chain {
    Chain {
        id: ChainId("legacy".into()),
        description: Some("migration roundtrip".into()),
        instrument: "electric_guitar".into(),
        enabled: true,
        volume: 100.0,
        blocks: vec![
            legacy_input("in:x", "dev_x"),
            legacy_output("out:x", "dev_x"),
        ],
    }
}

/// Pump `level` into input cpal 0, drain output route 0, return summed energy.
fn render_energy(runtimes: &[(usize, Arc<ChainRuntimeState>)], level: f32) -> f32 {
    assert!(!runtimes.is_empty(), "build produced no runtime");
    let runtime: Arc<ChainRuntimeState> = runtimes
        .iter()
        .find(|(_, rt)| rt.input_cpal_index() == Some(0))
        .map(|(_, rt)| rt.clone())
        .unwrap_or_else(|| runtimes[0].1.clone());
    let frames = 64usize;
    let data: Vec<f32> = vec![level; frames];
    for _ in 0..16 {
        process_input_f32(&runtime, 0, &data, 1);
    }
    let mut total = 0.0_f32;
    for _ in 0..16 {
        let mut out = vec![0.0_f32; frames];
        process_output_f32(&runtime, 0, &mut out, 1);
        total += out.iter().map(|s| s.abs()).sum::<f32>();
    }
    total
}

#[test]
fn migrated_chain_produces_same_audio_as_legacy_through_live_path() {
    // Legacy build (entries present, empty registry) — the byte-identical path.
    let legacy_req = BuildRequest {
        chain: legacy_chain(),
        sample_rate: 48_000.0,
        buffer_sizes: vec![1024],
        io_bindings: Vec::new(),
    };
    let legacy_rts = build_chain_runtime(&legacy_req).expect("legacy chain builds");
    let legacy_energy = render_energy(&legacy_rts, 0.5);
    assert!(
        legacy_energy > 1e-2,
        "legacy passthrough produced no energy ({legacy_energy:.6}) — test setup is wrong"
    );

    // Migrate: drains entries, sets io/endpoint, appends a binding.
    let mut project = Project {
        name: Some("roundtrip".into()),
        device_settings: Vec::new(),
        chains: vec![legacy_chain()],
        midi: None,
    };
    let mut bindings: Vec<IoBinding> = Vec::new();
    project::migrate_io_binding::migrate_legacy_io(&mut project, &mut bindings);

    // The migration must have actually drained entries and set io.
    let migrated_chain = project.chains[0].clone();
    let input_drained = migrated_chain.blocks.iter().any(|b| {
        matches!(&b.kind, AudioBlockKind::Input(ib) if ib.entries.is_empty() && !ib.io.is_empty())
    });
    assert!(
        input_drained,
        "migration must drain entries and set io on the input block"
    );
    assert!(!bindings.is_empty(), "migration must produce a binding");

    // Migrated build through the LIVE path with the migration's registry.
    let migrated_req = BuildRequest {
        chain: migrated_chain,
        sample_rate: 48_000.0,
        buffer_sizes: vec![1024],
        io_bindings: bindings,
    };
    let migrated_rts = build_chain_runtime(&migrated_req).expect("migrated chain builds");
    let migrated_energy = render_energy(&migrated_rts, 0.5);

    // C2 guard: the migrated chain must NOT be silent, and must match the
    // legacy energy (same passthrough, same level) within tolerance.
    assert!(
        migrated_energy > 1e-2,
        "C2: migrated chain is SILENT through the live path (energy={migrated_energy:.6})"
    );
    let rel = (migrated_energy - legacy_energy).abs() / legacy_energy.max(1e-6);
    assert!(
        rel < 1e-3,
        "migrated audio diverged from legacy: legacy={legacy_energy:.6} migrated={migrated_energy:.6} rel={rel:.6}"
    );
}
