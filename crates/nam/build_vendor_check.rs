//! Responsibility: tells a local build after a git fetch whether a newer NeuralAmpModelerCore exists.
//!
//! Issue #974. Only CI vendors a new NeuralAmpModelerCore (on `develop`, after
//! the tests pass). A local build just reports that one exists, so no working
//! copy ever holds a vendor commit its owner did not push, and every machine
//! builds the exact archive CI tested. A build script that reruns recompiles
//! every crate above `nam` (15 crates, 3.5 min measured), so the report cannot
//! ride on every build: it runs on the first build after a `git fetch` /
//! `git pull` (FETCH_HEAD changed), once per fetch. It never runs in CI, with
//! `CARGO_NET_OFFLINE`, or inside a cargo dependency checkout (OpenRig-plugins
//! builds `nam` from one; cargo marks it with `.cargo-ok` — its HEAD is a real
//! `master` branch, so HEAD tells nothing).
//!
//! Shared by `build.rs` and the crate's tests (`#[path]`), std only.
#![cfg_attr(test, allow(dead_code))]

use std::fs;
use std::path::{Path, PathBuf};
use std::process::Command;
use std::time::UNIX_EPOCH;

/// Why a build does not ask upstream.
#[derive(Debug, PartialEq, Eq)]
pub enum Skip {
    Ci,
    DependencyCheckout,
    Offline,
    NoFetch,
    AlreadyChecked,
}

/// Seconds a local build waits on upstream before giving up, unless
/// `NAM_VENDOR_TIMEOUT` says otherwise.
const LOCAL_TIMEOUT_SECS: &str = "20";

/// Whether this build asks upstream: `marker` names the latest fetch, `last`
/// the fetch the previous report ran for.
pub fn decide(
    ci: bool,
    dependency: bool,
    offline: bool,
    marker: Option<&str>,
    last: Option<&str>,
) -> Result<(), Skip> {
    if ci {
        return Err(Skip::Ci);
    }
    if dependency {
        return Err(Skip::DependencyCheckout);
    }
    if offline {
        return Err(Skip::Offline);
    }
    let Some(marker) = marker else {
        return Err(Skip::NoFetch);
    };
    if last == Some(marker) {
        return Err(Skip::AlreadyChecked);
    }
    Ok(())
}

/// Is `repo` a checkout cargo made for a dependency (git or registry)?
pub fn dependency_checkout(repo: &Path) -> bool {
    repo.join(".cargo-ok").exists()
}

/// The latest fetch, as FETCH_HEAD's modification time: every fetch rewrites
/// it, even one that brought nothing.
pub fn fetch_marker(git_dir: &Path) -> Option<String> {
    let modified = fs::metadata(git_dir.join("FETCH_HEAD"))
        .ok()?
        .modified()
        .ok()?;
    Some(
        modified
            .duration_since(UNIX_EPOCH)
            .ok()?
            .as_nanos()
            .to_string(),
    )
}

/// What a fetch rewrites, so its change reruns the build script: FETCH_HEAD,
/// or — in a fresh clone, which has none yet — the remote refs the first fetch
/// updates. Never a missing path: that makes cargo rerun it on every build.
pub fn rerun_path(git_dir: &Path) -> Option<PathBuf> {
    [git_dir.join("FETCH_HEAD"), git_dir.join("refs/remotes")]
        .into_iter()
        .find(|path| path.exists())
}

/// `build.rs` entry: registers the fetch trigger and, when a new fetch calls
/// for it, reports a newer upstream release for the checkout at `repo` as a
/// cargo warning. Never fails the build; `out_dir` remembers which fetch was
/// reported on.
pub fn check_after_fetch(repo: &Path, out_dir: &Path) {
    let Some(git_dir) = git_dir(repo) else {
        return;
    };
    if let Some(path) = rerun_path(&git_dir) {
        println!("cargo:rerun-if-changed={}", path.display());
    }
    let stamp = out_dir.join("nam-vendor-checked-fetch");
    let marker = fetch_marker(&git_dir);
    let last = fs::read_to_string(&stamp).ok();
    let offline = std::env::var("CARGO_NET_OFFLINE").is_ok_and(|v| v == "true" || v == "1");
    if decide(
        std::env::var_os("CI").is_some(),
        dependency_checkout(repo),
        offline,
        marker.as_deref(),
        last.as_deref(),
    )
    .is_err()
    {
        return;
    }
    report_newer_release(repo);
    if let Some(marker) = marker {
        let _ = fs::write(stamp, marker);
    }
}

fn git_dir(repo: &Path) -> Option<PathBuf> {
    let out = Command::new("git")
        .arg("-C")
        .arg(repo)
        .args(["rev-parse", "--absolute-git-dir"])
        .output()
        .ok()?;
    out.status
        .success()
        .then(|| PathBuf::from(String::from_utf8_lossy(&out.stdout).trim()))
}

/// A Python that actually runs (on Windows `python3` can be the Store's
/// placeholder, which exits non-zero).
fn python() -> Option<&'static str> {
    ["python3", "python"].into_iter().find(|python| {
        Command::new(python)
            .arg("--version")
            .output()
            .is_ok_and(|out| out.status.success())
    })
}

/// Run `scripts/nam_vendor.py check` and show what it found as cargo warnings.
fn report_newer_release(repo: &Path) {
    let script = repo.join("scripts/nam_vendor.py");
    if !script.exists() {
        return;
    }
    let Some(python) = python() else {
        println!("cargo:warning=NeuralAmpModelerCore release check skipped: no Python found");
        return;
    };
    let timeout =
        std::env::var("NAM_VENDOR_TIMEOUT").unwrap_or_else(|_| LOCAL_TIMEOUT_SECS.to_string());
    let Ok(out) = Command::new(python)
        .arg(&script)
        .arg("--repo")
        .arg(repo)
        .arg("check")
        .env("NAM_VENDOR_TIMEOUT", timeout)
        .output()
    else {
        return;
    };
    let text = format!(
        "{}{}",
        String::from_utf8_lossy(&out.stdout),
        String::from_utf8_lossy(&out.stderr)
    );
    for line in text
        .lines()
        .filter(|line| line.contains("available") || line.contains("warning"))
    {
        println!("cargo:warning={line}");
    }
}

#[cfg(test)]
#[path = "src/build_vendor_check_tests.rs"]
mod tests;
