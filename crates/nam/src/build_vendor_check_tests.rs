//! Tests for when a local build asks upstream whether a newer
//! NeuralAmpModelerCore exists (#974): only after a `git fetch`/`git pull`,
//! once per fetch, never in CI, offline, or inside a dependency checkout.

use std::fs;
use std::time::{Duration, SystemTime};

use super::{decide, dependency_checkout, fetch_marker, rerun_path, Skip};

#[test]
fn a_fetch_the_build_has_not_checked_yet_runs_the_check() {
    assert_eq!(decide(false, false, false, Some("2"), Some("1")), Ok(()));
    assert_eq!(decide(false, false, false, Some("1"), None), Ok(()));
}

#[test]
fn the_same_fetch_is_checked_once() {
    assert_eq!(
        decide(false, false, false, Some("1"), Some("1")),
        Err(Skip::AlreadyChecked)
    );
}

#[test]
fn no_fetch_yet_means_no_check() {
    assert_eq!(decide(false, false, false, None, None), Err(Skip::NoFetch));
}

#[test]
fn ci_never_checks_from_the_build() {
    // CI vendors on develop in its own job, after the tests.
    assert_eq!(
        decide(true, false, false, Some("2"), Some("1")),
        Err(Skip::Ci)
    );
}

#[test]
fn a_dependency_checkout_never_checks() {
    // OpenRig-plugins builds `nam` from a cargo git checkout: not ours to
    // report on, and it must build without the network.
    assert_eq!(
        decide(false, true, false, Some("2"), Some("1")),
        Err(Skip::DependencyCheckout)
    );
}

#[test]
fn an_offline_build_never_checks() {
    assert_eq!(
        decide(false, false, true, Some("2"), Some("1")),
        Err(Skip::Offline)
    );
}

#[test]
fn a_cargo_checkout_is_recognized_by_its_marker_not_its_head() {
    // Cargo's git checkouts sit on a real `master` branch, so HEAD tells
    // nothing; the `.cargo-ok` it writes at the checkout root does.
    let repo = tempfile::tempdir().unwrap();
    assert!(!dependency_checkout(repo.path()), "a plain clone is ours");
    fs::write(repo.path().join(".cargo-ok"), "").unwrap();
    assert!(dependency_checkout(repo.path()));
}

#[test]
fn a_missing_fetch_head_is_never_a_rerun_trigger() {
    // Cargo reruns a build script on every build when a rerun-if-changed path
    // does not exist — and each rerun of this one recompiles 15 crates.
    let git_dir = tempfile::tempdir().unwrap();
    assert_eq!(rerun_path(git_dir.path()), None);
    assert_eq!(fetch_marker(git_dir.path()), None);

    fs::write(git_dir.path().join("FETCH_HEAD"), "x").unwrap();
    assert_eq!(
        rerun_path(git_dir.path()),
        Some(git_dir.path().join("FETCH_HEAD"))
    );
}

#[test]
fn a_new_fetch_changes_the_marker_even_when_it_brought_nothing() {
    let git_dir = tempfile::tempdir().unwrap();
    let fetch_head = git_dir.path().join("FETCH_HEAD");
    fs::write(&fetch_head, "same").unwrap();
    let file = fs::File::options().write(true).open(&fetch_head).unwrap();
    file.set_modified(SystemTime::UNIX_EPOCH + Duration::from_secs(1_000))
        .unwrap();
    let first = fetch_marker(git_dir.path()).expect("a fetch happened");

    file.set_modified(SystemTime::UNIX_EPOCH + Duration::from_secs(2_000))
        .unwrap();
    let second = fetch_marker(git_dir.path()).expect("a fetch happened");

    assert_ne!(first, second, "a second fetch is a new chance to check");
}

#[test]
fn a_fresh_clone_still_notices_its_first_fetch() {
    // `git clone` writes no FETCH_HEAD. Watching nothing would leave the
    // report dead in every fresh clone until something else reran the build
    // script; the remote refs a first fetch rewrites are watched instead.
    let git_dir = tempfile::tempdir().unwrap();
    let remotes = git_dir.path().join("refs/remotes");
    fs::create_dir_all(&remotes).unwrap();

    assert_eq!(rerun_path(git_dir.path()), Some(remotes));
}
