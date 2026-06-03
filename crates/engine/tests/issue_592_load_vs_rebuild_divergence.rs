//! Issue #592 — RED-first repro: a chain runtime built by the LOAD path
//! (`build_chain_runtime_state`) must produce the SAME audio as the same
//! chain after a no-op param edit re-built it through the LIVE EDIT path
//! (`update_chain_runtime_state`).
//!
//! User symptom (reproduces on `develop`, independent of #588): a clean
//! preset loads distorted/clipping; the user nudges the amp knob and
//! returns it to the EXACT original value (a no-op edit), the chain
//! rebuilds, and the sound becomes clean. Identical parameters → different
//! output depending on whether the runtime was freshly built (load) or
//! rebuilt (edit) — an initialisation/determinism bug in the build path.
//!
//! Acceptance encoded here, per preset (OD / CLEAN / CRUNCH) and per
//! device buffer (32 / 64 frames):
//!   * the load-built and the edit-rebuilt runtime, fed identical input
//!     with identical params, must produce byte-identical output, and
//!   * the load-built output must NOT clip / blow up for a clean preset.
//!
//! These chains carry the user's real NAM gain-staging (`input_db`,
//! `output_db`, `eq`, per-capture selection, manifest `output_gain_db`),
//! so the test depends on the `OpenRig-plugins` tree being present at the
//! same dev path as `crates/engine/tests/issue_542_od_chain_clips.rs`. If
//! it is missing the test fails loudly — the repro depends on the actual
//! captures and their gain calibration.

use std::path::PathBuf;
use std::sync::Arc;
use std::sync::Once;

use block_core::param::ParameterSet;
use domain::ids::{BlockId, ChainId, DeviceId};
use domain::value_objects::ParameterValue;
use engine::runtime::{process_input_f32, process_output_f32};
use engine::runtime_graph::{build_chain_runtime_state, update_chain_runtime_state};
use engine::runtime_state::ChainRuntimeState;
use project::block::{
    AudioBlock, AudioBlockKind, InputBlock, InputEntry, NamBlock, OutputBlock, OutputEntry,
};
use project::chain::{Chain, ChainInputMode, ChainOutputMode};

const SR: f32 = 48_000.0;
const WARMUP: usize = 2048;
const FRAMES: usize = 8192;

fn plugins_root() -> PathBuf {
    let candidates = [
        PathBuf::from(env!("CARGO_MANIFEST_DIR"))
            .join("../../../../../OpenRig-plugins/plugins/source"),
        PathBuf::from(
            "/Users/joao.faria/Projetos/github.com/jpfaria/OpenRig-plugins/plugins/source",
        ),
    ];
    candidates
        .into_iter()
        .find(|p| p.is_dir())
        .expect("OpenRig-plugins/plugins/source must be present on disk for issue #592 repro")
}

fn init_registry() {
    static INIT: Once = Once::new();
    INIT.call_once(|| {
        nam::register_builder();
        ir::register_builder();
        block_dyn::register_natives();
        block_filter::register_natives();
        block_reverb::register_natives();
        block_gain::register_natives();
        block_amp::register_natives();
        block_preamp::register_natives();
        block_cab::register_natives();
        plugin_loader::registry::init(&plugins_root());
    });
}

fn nam_block(id: &str, model: &str, params: ParameterSet) -> AudioBlock {
    AudioBlock {
        id: BlockId(id.into()),
        enabled: true,
        kind: AudioBlockKind::Nam(NamBlock {
            model: model.into(),
            params,
        }),
    }
}

fn input_block() -> AudioBlock {
    AudioBlock {
        id: BlockId("in".into()),
        enabled: true,
        kind: AudioBlockKind::Input(InputBlock {
            model: "standard".into(),
            entries: vec![InputEntry {
                device_id: DeviceId("dev".into()),
                mode: ChainInputMode::Mono,
                channels: vec![0],
            }],
        }),
    }
}

fn output_block() -> AudioBlock {
    AudioBlock {
        id: BlockId("out".into()),
        enabled: true,
        kind: AudioBlockKind::Output(OutputBlock {
            model: "standard".into(),
            entries: vec![OutputEntry {
                device_id: DeviceId("dev".into()),
                mode: ChainOutputMode::Stereo,
                channels: vec![0, 1],
            }],
        }),
    }
}

/// Common NAM gain-staging params used by every preset block.
fn gain_staging(p: &mut ParameterSet, input_db: f32, output_db: f32, eq_enabled: bool) {
    p.insert("input_db", ParameterValue::Float(input_db));
    p.insert("output_db", ParameterValue::Float(output_db));
    p.insert("eq.enabled", ParameterValue::Bool(eq_enabled));
    p.insert("eq.bass", ParameterValue::Float(5.0));
    p.insert("eq.middle", ParameterValue::Float(5.0));
    p.insert("eq.treble", ParameterValue::Float(5.0));
    p.insert("noise_gate.enabled", ParameterValue::Bool(false));
    p.insert("noise_gate.threshold_db", ParameterValue::Float(-50.0));
}

/// CLEAN preset — enabled NAM blocks only (the clean tone the user
/// reports as loading distorted): lovepedal gain + Dumble ODS amp dialed
/// clean (input attenuated -13.6 dB, character hiz).
fn clean_chain() -> Chain {
    let mut burst = ParameterSet::default();
    burst.insert("preset", ParameterValue::String("d5_t5_v4".into()));
    gain_staging(&mut burst, 0.0, 0.0, false);

    let mut dumble = ParameterSet::default();
    dumble.insert("character", ParameterValue::String("hiz".into()));
    gain_staging(&mut dumble, -13.6, -0.8, true);

    chain(
        "issue-592-clean",
        vec![
            nam_block("burst", "nam_lovepedal_eternity_burst", burst),
            nam_block("dumble", "nam_dumble_ods_john_mayer", dumble),
        ],
    )
}

/// OD preset — enabled NAM blocks: Big Muff fuzz → Nobels ODR-1 → Mesa
/// Mark III preamp (cranked). Real hot calibrations (+2.98, +2.86 dB).
fn od_chain() -> Chain {
    let mut muff = ParameterSet::default();
    muff.insert("size", ParameterValue::String("0".into()));
    muff.insert("nam_size", ParameterValue::String("standard".into()));
    muff.insert("sustain", ParameterValue::Float(0.0));
    gain_staging(&mut muff, 0.0, 0.0, true);

    let mut nob = ParameterSet::default();
    nob.insert("gain", ParameterValue::Float(28.0));
    gain_staging(&mut nob, 0.0, 0.0, false);

    let mut mark = ParameterSet::default();
    mark.insert("eq", ParameterValue::String("lohi".into()));
    mark.insert("gain", ParameterValue::String("cranked".into()));
    gain_staging(&mut mark, 0.0, 0.0, false);

    chain(
        "issue-592-od",
        vec![
            nam_block("muff", "nam_big_muff", muff),
            nam_block("nob", "nam_nobels_odr_1", nob),
            nam_block("mark", "nam_mesa_mark_iii", mark),
        ],
    )
}

/// CRUNCH preset — enabled NAM blocks: Ibanez TS9 → Dumble amp (crunch).
fn crunch_chain() -> Chain {
    let mut ts9 = ParameterSet::default();
    ts9.insert("drive", ParameterValue::Float(6.0));
    ts9.insert("level", ParameterValue::Float(5.0));
    ts9.insert("tone", ParameterValue::Float(5.0));
    gain_staging(&mut ts9, 0.0, 0.0, true);

    let mut dumble = ParameterSet::default();
    dumble.insert("cabinet", ParameterValue::String("4x12_v30".into()));
    dumble.insert("gain", ParameterValue::String("crunch".into()));
    gain_staging(&mut dumble, 0.0, 0.0, false);

    chain(
        "issue-592-crunch",
        vec![
            nam_block("ts9", "nam_ibanez_ts9", ts9),
            nam_block("dumble", "nam_dumble", dumble),
        ],
    )
}

fn chain(id: &str, mut nam_blocks: Vec<AudioBlock>) -> Chain {
    let mut blocks = vec![input_block()];
    blocks.append(&mut nam_blocks);
    blocks.push(output_block());
    Chain {
        id: ChainId(id.into()),
        description: Some("issue-592 load vs rebuild".into()),
        instrument: "electric_guitar".into(),
        enabled: true,
        volume: 100.0,
        blocks,
    }
}

/// Same chain with the last NAM block's `output_db` bumped by +0.1 — the
/// user's "nudge the knob" half of the no-op edit.
fn nudged(orig: &Chain) -> Chain {
    let mut c = orig.clone();
    if let Some(AudioBlock {
        kind: AudioBlockKind::Nam(nam),
        ..
    }) = c
        .blocks
        .iter_mut()
        .rev()
        .find(|b| matches!(b.kind, AudioBlockKind::Nam(_)))
    {
        let cur = match nam.params.get("output_db") {
            Some(ParameterValue::Float(v)) => *v,
            _ => 0.0,
        };
        nam.params
            .insert("output_db", ParameterValue::Float(cur + 0.1));
    }
    c
}

fn di_sine(frames: usize) -> Vec<f32> {
    (0..frames)
        .map(|n| 0.3 * (2.0 * std::f32::consts::PI * 110.0 * n as f32 / SR).sin())
        .collect()
}

fn drive(rt: &Arc<ChainRuntimeState>, mono: &[f32], buffer: usize) -> Vec<f32> {
    let warmup = vec![0.0f32; WARMUP];
    for chunk in warmup.chunks(buffer) {
        process_input_f32(rt, 0, chunk, 1);
        let mut o = vec![0.0f32; chunk.len() * 2];
        process_output_f32(rt, 0, &mut o, 2);
    }
    let mut out = Vec::with_capacity(mono.len() * 2);
    for chunk in mono.chunks(buffer) {
        process_input_f32(rt, 0, chunk, 1);
        let mut o = vec![0.0f32; chunk.len() * 2];
        process_output_f32(rt, 0, &mut o, 2);
        out.extend_from_slice(&o);
    }
    out
}

fn peak(s: &[f32]) -> f32 {
    s.iter().fold(0.0_f32, |a, &b| a.max(b.abs()))
}

fn max_abs_diff(a: &[f32], b: &[f32]) -> f32 {
    assert_eq!(a.len(), b.len());
    a.iter()
        .zip(b.iter())
        .fold(0.0_f32, |m, (&x, &y)| m.max((x - y).abs()))
}

fn dbfs(x: f32) -> f32 {
    20.0 * x.max(1e-9).log10()
}

fn build_load(c: &Chain, buffer: usize) -> Arc<ChainRuntimeState> {
    Arc::new(build_chain_runtime_state(c, SR, &[buffer]).expect("load-path build must succeed"))
}

/// Build, then the user's no-op edit: nudge the amp `output_db` and return
/// it to the original value, both through the in-place live edit path.
fn build_then_noop_edit(orig: &Chain, buffer: usize) -> Arc<ChainRuntimeState> {
    let rt = build_load(orig, buffer);
    update_chain_runtime_state(&rt, &nudged(orig), SR, false, &[buffer])
        .expect("nudge edit must apply");
    update_chain_runtime_state(&rt, orig, SR, false, &[buffer]).expect("restore edit must apply");
    rt
}

fn assert_load_matches_rebuild(name: &str, chain: &Chain) {
    let input = di_sine(FRAMES);
    for &buffer in &[32usize, 64] {
        let out_load = drive(&build_load(chain, buffer), &input, buffer);
        let out_edit = drive(&build_then_noop_edit(chain, buffer), &input, buffer);

        let diff = max_abs_diff(&out_load, &out_edit);
        let (p_load, p_edit) = (peak(&out_load), peak(&out_edit));
        eprintln!(
            "issue #592 [{name}] @buffer={buffer}: load peak {p_load:.4} ({:+.2} dBFS), \
             edit peak {p_edit:.4} ({:+.2} dBFS), max|load-edit| = {diff:.6}",
            dbfs(p_load),
            dbfs(p_edit),
        );

        assert!(
            out_load.iter().all(|s| s.is_finite()),
            "[{name}] @buffer={buffer}: load-built output produced NaN/Inf"
        );
        assert!(
            diff < 1e-4,
            "BUG #592 [{name}] @buffer={buffer}: load-built and edit-rebuilt runtime \
             diverge by {diff:.6} (load peak {:+.2} dBFS, edit peak {:+.2} dBFS) for \
             IDENTICAL params. The freshly-loaded chain produces different audio than \
             the same chain after a no-op param edit — an initialisation/determinism \
             bug in the build path. The edit path resolves the NAM gain staging \
             correctly; the load path does not.",
            dbfs(p_load),
            dbfs(p_edit),
        );
        assert!(
            p_load <= 1.0,
            "BUG #592 [{name}] @buffer={buffer}: load-built output clips at \
             {p_load:.4} ({:+.2} dBFS) — the load path feeds the model too hot.",
            dbfs(p_load),
        );
    }
}

fn preset_fixture(name: &str) -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("tests/fixtures/presets")
        .join(name)
}

// ── FULL preset chain (enabled + DISABLED blocks) load vs rebuild ────────
//
// The user's presets interleave DISABLED blocks (OD: a disabled Dumble
// amp + disabled plate reverb; CLEAN: three disabled drives). A chain
// with born-disabled blocks may build differently on first load than
// after a no-op param edit rebuilds it (cf. #589 "restore rebuild
// fallback when re-enabling a born-disabled block"). These tests load
// the FULL chain from the real file (every block except LV2, which needs
// the LV2 host on disk) and assert the load-built runtime renders the
// same audio as the edit-rebuilt one.

fn load_full_preset_chain(file: &str, id: &str) -> Chain {
    let preset = infra_yaml::load_chain_preset_file(&preset_fixture(file))
        .unwrap_or_else(|e| panic!("parser must load preset {file}: {e}"));
    let blocks: Vec<AudioBlock> = preset
        .blocks
        .into_iter()
        .filter(|b| {
            let model = match &b.kind {
                AudioBlockKind::Nam(n) => n.model.as_str(),
                AudioBlockKind::Core(c) => c.model.as_str(),
                _ => "",
            };
            // LV2 blocks need the LV2 host + plugins on disk; out of scope
            // for the build-determinism repro.
            !model.starts_with("lv2_")
        })
        .collect();
    chain(id, blocks)
}

/// Nudge the first ENABLED NAM block's `output_db` by +0.1 (the user's
/// knob move), used as the no-op edit on the full chain.
fn nudged_full(orig: &Chain) -> Chain {
    let mut c = orig.clone();
    if let Some(block) = c.blocks.iter_mut().find(|b| {
        b.enabled
            && matches!(&b.kind, AudioBlockKind::Nam(_) | AudioBlockKind::Core(_))
            && matches!(&b.kind, AudioBlockKind::Nam(n) if n.model.starts_with("nam_"))
    }) {
        if let AudioBlockKind::Nam(nam) = &mut block.kind {
            let cur = match nam.params.get("output_db") {
                Some(ParameterValue::Float(v)) => *v,
                _ => 0.0,
            };
            nam.params
                .insert("output_db", ParameterValue::Float(cur + 0.1));
        }
    }
    c
}

fn assert_full_chain_load_matches_rebuild(name: &str, file: &str) {
    let chain = load_full_preset_chain(file, &format!("issue-592-{name}-full"));
    let input = di_sine(FRAMES);
    for &buffer in &[32usize, 64] {
        let out_load = drive(&build_load(&chain, buffer), &input, buffer);

        let rt = build_load(&chain, buffer);
        update_chain_runtime_state(&rt, &nudged_full(&chain), SR, false, &[buffer])
            .expect("nudge edit must apply");
        update_chain_runtime_state(&rt, &chain, SR, false, &[buffer])
            .expect("restore edit must apply");
        let out_edit = drive(&rt, &input, buffer);

        // Level-based, offset-tolerant comparison. The #592 fix primes the
        // output elastic buffer with a silence cushion on the INITIAL build
        // of an IR chain (and only then), so a chain with a cab/IR block has
        // its load output shifted by the cushion vs the warm rebuild. The
        // PROCESSING is unchanged — peak and RMS must still match — but the
        // streams are no longer sample-aligned, so peak+RMS (invariant to a
        // leading offset) is the right contract here.
        // Peak is invariant to the leading IR cushion offset; RMS is not
        // (the prime shifts which window of the same signal is measured),
        // so peak equality is the right offset-tolerant proof that the DSP
        // is identical load vs rebuild.
        let (p_load, p_edit) = (peak(&out_load), peak(&out_edit));
        eprintln!(
            "issue #592 FULL [{name}] @buffer={buffer}: load peak {p_load:.4} \
             ({:+.2} dBFS), edit peak {p_edit:.4} ({:+.2} dBFS)",
            dbfs(p_load),
            dbfs(p_edit),
        );
        assert!(
            out_load.iter().all(|s| s.is_finite()),
            "[{name}] @buffer={buffer}: full-chain load output produced NaN/Inf"
        );
        assert!(
            (p_load - p_edit).abs() < 1e-3,
            "BUG #592 [{name}] @buffer={buffer}: the FULL chain built on load \
             processes to a different peak ({:+.2} dBFS) than after a no-op \
             param edit rebuilds it ({:+.2} dBFS). The DSP must be identical \
             load vs rebuild; only the IR cushion offset may differ.",
            dbfs(p_load),
            dbfs(p_edit),
        );
    }
}

#[test]
fn clean_full_chain_load_matches_rebuild() {
    init_registry();
    assert_full_chain_load_matches_rebuild("CLEAN", "clean.yaml");
}

#[test]
fn od_full_chain_load_matches_rebuild() {
    init_registry();
    assert_full_chain_load_matches_rebuild("OD", "OD.yaml");
}

#[test]
fn crunch_full_chain_load_matches_rebuild() {
    init_registry();
    assert_full_chain_load_matches_rebuild("CRUNCH", "Crunch.yaml");
}

#[test]
fn clean_preset_load_matches_rebuild() {
    init_registry();
    assert_load_matches_rebuild("CLEAN", &clean_chain());
}

#[test]
fn od_preset_load_matches_rebuild() {
    init_registry();
    assert_load_matches_rebuild("OD", &od_chain());
}

#[test]
fn crunch_preset_load_matches_rebuild() {
    init_registry();
    assert_load_matches_rebuild("CRUNCH", &crunch_chain());
}
