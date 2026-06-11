//! Issue #692 — opening a project takes far too long even though the
//! project file is tiny (12 KB YAML). Expected contract: project open is
//! parse-into-memory only; heavy DSP work (NAM weights, LV2
//! instantiation, IR convolver setup, stream rebuild) belongs to chain
//! start, not to open.
//!
//! Phase 1 probe (layer 1 of the open path): time
//! `load_project_session` against a copy of the user's real-world
//! project (multi-chain, NAM A2 + LV2 + IR blocks, 158 KB `.openrig`
//! preset bank). RED if this layer alone blows the parse budget; GREEN
//! here means the stall lives further down the open wiring and the next
//! probe moves there.
//!
//! The repro copies the real files from `~/.openrig` /
//! `~/Library/Application Support/OpenRig` into a tempdir so the run
//! never mutates user state. Skips (passes trivially) when the fixture
//! files are absent, e.g. on CI.

use std::path::PathBuf;
use std::time::{Duration, Instant};

use tempfile::TempDir;

use crate::project_ops::load_project_session;

/// Generous ceiling for "read a 12 KB YAML + 158 KB preset bank into
/// memory". Parsing alone sits in the microsecond range; anything past
/// this budget means open is doing work that belongs to chain start.
const OPEN_PARSE_BUDGET: Duration = Duration::from_millis(200);

#[test]
fn issue_692_load_project_session_stays_within_parse_budget() {
    let home = match std::env::var("HOME") {
        Ok(h) => PathBuf::from(h),
        Err(_) => return,
    };
    let real_project = home.join(".openrig/project.yaml");
    let real_openrig = home.join(".openrig/project.openrig");
    let real_cfg = home.join("Library/Application Support/OpenRig/config.yaml");
    if !real_project.exists() || !real_cfg.exists() {
        eprintln!("issue_692: real-world fixture not present, skipping");
        return;
    }

    let tmp = TempDir::new().expect("tempdir");
    let project = tmp.path().join("project.yaml");
    let cfg = tmp.path().join("config.yaml");
    std::fs::copy(&real_project, &project).expect("copy project.yaml");
    std::fs::copy(&real_cfg, &cfg).expect("copy config.yaml");
    if real_openrig.exists() {
        std::fs::copy(&real_openrig, tmp.path().join("project.openrig")).expect("copy .openrig");
    }

    let t0 = Instant::now();
    let session = load_project_session(&project, &cfg).expect("load_project_session");
    let elapsed = t0.elapsed();
    eprintln!(
        "issue_692: load_project_session took {elapsed:?} ({} chains)",
        session.project.borrow().chains.len()
    );

    assert!(
        elapsed < OPEN_PARSE_BUDGET,
        "project open (load_project_session) took {elapsed:?}, budget is {OPEN_PARSE_BUDGET:?} — \
         open must be parse-only; heavy DSP setup belongs to chain start (issue #692)"
    );
}

/// Phase 1 probe (layer 2 of the open path): time the full runtime start
/// (`ProjectRuntimeController::start`) for the same real-world project —
/// this is the NAM/LV2/IR build + stream bring-up that runs when chains
/// go live. Prints per-stage timings; RED if the whole bring-up exceeds
/// the budget, localizing the user-visible stall.
#[test]
fn issue_692_runtime_start_probe() {
    // Real-hardware battery gate: opens the physical audio interface.
    if std::env::var_os("OPENRIG_HW_TESTS").is_none() {
        eprintln!("issue_692: SKIPPED — set OPENRIG_HW_TESTS=1 on an idle machine");
        return;
    }
    let home = match std::env::var("HOME") {
        Ok(h) => PathBuf::from(h),
        Err(_) => return,
    };
    let real_project = home.join(".openrig/project.yaml");
    let real_cfg = home.join("Library/Application Support/OpenRig/config.yaml");
    let user_plugins = home
        .join("Projetos/github.com/jpfaria/OpenRig-plugins/plugins/source");
    if !real_project.exists() || !real_cfg.exists() || !user_plugins.exists() {
        eprintln!("issue_692: real-world fixture not present, skipping");
        return;
    }

    let t0 = Instant::now();
    plugin_loader::registry::init_many(&[PathBuf::from("plugins"), user_plugins]);
    eprintln!("issue_692: registry init took {:?}", t0.elapsed());

    let tmp = TempDir::new().expect("tempdir");
    let project_path = tmp.path().join("project.yaml");
    let cfg = tmp.path().join("config.yaml");
    std::fs::copy(&real_project, &project_path).expect("copy project.yaml");
    std::fs::copy(&real_cfg, &cfg).expect("copy config.yaml");
    let real_openrig = home.join(".openrig/project.openrig");
    if real_openrig.exists() {
        std::fs::copy(&real_openrig, tmp.path().join("project.openrig")).expect("copy .openrig");
    }

    let session = load_project_session(&project_path, &cfg).expect("load_project_session");
    let mut project = session.project.borrow().clone();
    // Mirror the user flipping every chain on (the rig going live).
    for chain in &mut project.chains {
        chain.enabled = true;
    }
    let enabled_chains = project.chains.iter().filter(|c| c.enabled).count();

    let t1 = Instant::now();
    let controller =
        infra_cpal::ProjectRuntimeController::start(&project).expect("runtime start");
    let elapsed = t1.elapsed();
    eprintln!(
        "issue_692: ProjectRuntimeController::start took {elapsed:?} \
         ({enabled_chains} enabled chains of {})",
        project.chains.len()
    );
    drop(controller);

    assert!(
        elapsed < Duration::from_millis(500),
        "runtime start took {elapsed:?} for {enabled_chains} enabled chain(s) — \
         this is the user-visible stall when opening/starting the rig (issue #692)"
    );
}
