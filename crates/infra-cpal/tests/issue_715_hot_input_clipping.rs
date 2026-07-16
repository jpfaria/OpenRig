//! THE ORIGINAL problem (2026-06-17): "o som está clipando ... estalos como se
//! tivesse em curto." The user's high-gain guitar chains (TS9 drive 10 ->
//! Dumble -> JCM800 preamp; Klon -> Dumble) stack a lot of gain. A moderate DI
//! renders clean (peak 0.76), but a HOT input — hard palm-muted chords near
//! full scale — can push the chain output past the limiter into HARD digital
//! clipping (samples pinned at the +-1.0 rail, a buzzy "short-circuit" sound,
//! distinct from the smooth tanh amp saturation).
//!
//! This renders the user's REAL guitar chains OFFLINE (real full-size NAM
//! captures from their plugins tree) with the real DI NORMALISED HOT (peak
//! ~0.95) and scans the output for hard clipping (sustained runs at the rail),
//! out-of-range samples, and NaN. Deterministic, no device, no GUI, no ear.
#![cfg(target_os = "macos")]

mod hw_harness;

use std::path::PathBuf;

use hw_harness::init_registry_with_root;
use project::block::AudioBlockKind;
use project::chain::Chain;

/// The owner's private capture tree, from `OPENRIG_OWNER_PLUGINS` or a sibling
/// `OpenRig-plugins` checkout at any depth. `None` when absent (the test skips).
fn owner_plugins_root() -> Option<PathBuf> {
    if let Some(p) = std::env::var_os("OPENRIG_OWNER_PLUGINS") {
        let p = PathBuf::from(p);
        if p.is_dir() {
            return Some(p);
        }
    }
    let mut dir = PathBuf::from(env!("CARGO_MANIFEST_DIR"));
    loop {
        let cand = dir.join("OpenRig-plugins/plugins/source");
        if cand.is_dir() {
            return Some(cand);
        }
        if !dir.pop() {
            return None;
        }
    }
}

/// The owner's real rig, from `OPENRIG_OWNER_PROJECT`. This diagnostic renders
/// the owner's ACTUAL chains, so the project is opt-in and never read from a
/// hardcoded live path — `None` (skip) unless the owner points it explicitly.
fn owner_project_path() -> Option<PathBuf> {
    std::env::var_os("OPENRIG_OWNER_PROJECT")
        .map(PathBuf::from)
        .filter(|p| p.is_file())
}

fn di_path() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("../../assets/di-loops/phil-STRATO-green_day.wav")
}

fn is_guitar(chain: &Chain) -> bool {
    chain.instrument == "electric_guitar"
}

/// Load the DI mono and NORMALISE it so its peak is `target` — a hot,
/// hard-played guitar level.
fn load_di_hot(target: f32) -> Vec<[f32; 2]> {
    let mut reader = hound::WavReader::open(di_path()).expect("open DI");
    let spec = reader.spec();
    let ch = spec.channels as usize;
    let raw: Vec<f32> = match spec.sample_format {
        hound::SampleFormat::Float => reader.samples::<f32>().map(|s| s.unwrap()).collect(),
        hound::SampleFormat::Int => {
            let max = (1i64 << (spec.bits_per_sample - 1)) as f32;
            reader
                .samples::<i32>()
                .map(|s| s.unwrap() as f32 / max)
                .collect()
        }
    };
    let mono: Vec<f32> = raw.chunks(ch).map(|c| c[0]).collect();
    let peak = mono.iter().fold(0.0_f32, |a, &b| a.max(b.abs())).max(1e-9);
    let g = target / peak;
    mono.iter()
        .map(|&s| {
            let v = s * g;
            [v, v]
        })
        .collect()
}

struct Scan {
    peak: f32,
    over_one: usize,
    rail_run_max: usize,
    nan: usize,
}

fn scan(samples: &[[f32; 2]]) -> Scan {
    let mut s = Scan {
        peak: 0.0,
        over_one: 0,
        rail_run_max: 0,
        nan: 0,
    };
    for ch in 0..2 {
        let mut run = 0usize;
        for fr in samples {
            let v = fr[ch];
            if !v.is_finite() {
                s.nan += 1;
                run = 0;
                continue;
            }
            s.peak = s.peak.max(v.abs());
            if v.abs() > 1.0 {
                s.over_one += 1;
            }
            if v.abs() >= 0.9995 {
                run += 1;
                s.rail_run_max = s.rail_run_max.max(run);
            } else {
                run = 0;
            }
        }
    }
    s
}

#[test]
fn user_guitar_chains_do_not_hard_clip_on_a_hot_input() {
    let Some(root) = owner_plugins_root() else {
        eprintln!("[#715-clip] owner plugins tree not present (set OPENRIG_OWNER_PLUGINS) — skipping");
        return;
    };
    init_registry_with_root(&root);

    let Some(project_path) = owner_project_path() else {
        eprintln!("[#715-clip] owner project not set (OPENRIG_OWNER_PROJECT=<project.yaml>) — skipping");
        return;
    };
    let rig = infra_yaml::load_project_any(&project_path).expect("load owner project");
    let enabled: std::collections::BTreeSet<String> = rig.inputs.keys().cloned().collect();
    let project = engine::rig_runtime::rig_to_legacy_project(&rig, &enabled);

    let di_hot = load_di_hot(0.95); // hard-played level
    let di_sr = {
        let r = hound::WavReader::open(di_path()).unwrap();
        r.spec().sample_rate
    };

    // The user's real chains now carry catalog/system VST3 blocks (ChowCentaur,
    // ValhallaSupermassive — issue #776). The offline render resolves them from
    // the VST3 catalog, which the live app builds at startup; without this the
    // blocks fault "not found in catalog", every guitar chain is skipped, and
    // the test can't measure clipping at all. Initialise it the same way.
    project::vst3_editor::init_vst3_catalog(di_sr as f64, &[root.clone()]);

    let mut worst: Option<(String, Scan)> = None;
    for chain in project.chains.into_iter().filter(is_guitar) {
        let n_nam = chain
            .blocks
            .iter()
            .filter(|b| {
                matches!(&b.kind, AudioBlockKind::Core(c) if c.model.starts_with("nam_"))
                    || matches!(b.kind, AudioBlockKind::Nam(_))
            })
            .count();
        let outcome = match engine::offline::render_chain(
            &chain,
            di_sr as f32,
            &di_hot,
            64,
            di_sr as usize,
        ) {
            Ok(o) => o,
            Err(e) => {
                eprintln!("[#715-clip] chain '{}' did not render: {e}", chain.id.0);
                continue;
            }
        };
        if !outcome.faulted_blocks.is_empty() {
            eprintln!(
                "[#715-clip] chain '{}' has faulted blocks {:?} — skipping",
                chain.id.0, outcome.faulted_blocks
            );
            continue;
        }
        let s = scan(&outcome.samples);
        eprintln!(
            "[#715-clip] chain '{}' ({n_nam} NAM): peak={:.4} over1.0={} rail_run_max={} nan={}",
            chain.id.0, s.peak, s.over_one, s.rail_run_max, s.nan
        );
        if worst
            .as_ref()
            .is_none_or(|(_, w)| s.rail_run_max > w.rail_run_max)
        {
            worst = Some((chain.id.0.clone(), s));
        }
    }

    let (cid, s) = worst.expect("at least one guitar chain must render");
    assert_eq!(s.nan, 0, "non-finite samples in chain '{cid}'");
    assert_eq!(
        s.over_one, 0,
        "chain '{cid}': {} samples exceed +-1.0 — the limiter is not bounding the output",
        s.over_one
    );
    assert!(
        s.rail_run_max < 8,
        "REPRODUCED: chain '{cid}' HARD-CLIPS on a hot input — {} consecutive samples \
         pinned at the +-1.0 rail (peak {:.4}). That is the buzzy 'clipando / em curto', \
         distinct from the musical tanh limiter. A gain stage before the limiter is \
         railing the signal.",
        s.rail_run_max,
        s.peak
    );
}
