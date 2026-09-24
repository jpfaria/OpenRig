//! Tests for the vendored NeuralAmpModelerCore extraction (#974).

use std::fs;
use std::path::Path;

use super::{ensure_vendor, extraction_lock, is_lfs_pointer, Outcome, STAMP};

/// A tar.gz holding `NeuralAmpModelerCore/NAM/dsp.cpp` with `body`.
fn archive(dir: &Path, body: &str) -> std::path::PathBuf {
    let path = dir.join("NeuralAmpModelerCore.tar.gz");
    let file = fs::File::create(&path).unwrap();
    let gz = flate2::write::GzEncoder::new(file, flate2::Compression::fast());
    let mut tar = tar::Builder::new(gz);
    let mut header = tar::Header::new_gnu();
    header.set_size(body.len() as u64);
    header.set_mode(0o644);
    header.set_cksum();
    tar.append_data(
        &mut header,
        "NeuralAmpModelerCore/NAM/dsp.cpp",
        body.as_bytes(),
    )
    .unwrap();
    tar.into_inner().unwrap().finish().unwrap();
    path
}

fn lock(dir: &Path, text: &str) -> std::path::PathBuf {
    let path = dir.join("NeuralAmpModelerCore.lock");
    fs::write(&path, text).unwrap();
    path
}

#[test]
fn a_missing_tree_is_extracted_from_the_archive() {
    let dir = tempfile::tempdir().unwrap();
    let archive = archive(dir.path(), "v1");
    let lock = lock(dir.path(), "v0.5.4\n");
    let dest = dir.path().join("NeuralAmpModelerCore");

    assert_eq!(
        ensure_vendor(&archive, &lock, &dest),
        Ok(Outcome::Extracted)
    );
    assert_eq!(fs::read_to_string(dest.join("NAM/dsp.cpp")).unwrap(), "v1");
    assert_eq!(fs::read_to_string(dest.join(STAMP)).unwrap(), "v0.5.4\n");
}

#[test]
fn a_tree_extracted_from_the_same_lock_is_left_alone() {
    let dir = tempfile::tempdir().unwrap();
    let archive = archive(dir.path(), "v1");
    let lock = lock(dir.path(), "v0.5.4\n");
    let dest = dir.path().join("NeuralAmpModelerCore");
    ensure_vendor(&archive, &lock, &dest).unwrap();
    fs::write(dest.join("marker"), "kept").unwrap();

    assert_eq!(ensure_vendor(&archive, &lock, &dest), Ok(Outcome::Present));
    assert!(dest.join("marker").exists(), "nothing was re-extracted");
}

#[test]
fn a_new_lock_replaces_the_whole_tree() {
    let dir = tempfile::tempdir().unwrap();
    let dest = dir.path().join("NeuralAmpModelerCore");
    ensure_vendor(
        &archive(dir.path(), "v1"),
        &lock(dir.path(), "v0.5.4\n"),
        &dest,
    )
    .unwrap();
    fs::write(dest.join("stale"), "old").unwrap();

    let outcome = ensure_vendor(
        &archive(dir.path(), "v2"),
        &lock(dir.path(), "v0.5.5\n"),
        &dest,
    );

    assert_eq!(outcome, Ok(Outcome::Extracted));
    assert_eq!(fs::read_to_string(dest.join("NAM/dsp.cpp")).unwrap(), "v2");
    assert!(
        !dest.join("stale").exists(),
        "files of the old version are gone"
    );
}

#[test]
fn an_lfs_pointer_is_recognized() {
    let dir = tempfile::tempdir().unwrap();
    let pointer = dir.path().join("pointer.tar.gz");
    fs::write(
        &pointer,
        "version https://git-lfs.github.com/spec/v1\noid sha256:abc\nsize 42\n",
    )
    .unwrap();
    assert!(is_lfs_pointer(&pointer));
    assert!(!is_lfs_pointer(&archive(dir.path(), "v1")));
}

#[test]
fn an_unfetched_lfs_archive_fails_with_the_command_that_fixes_it() {
    let dir = tempfile::tempdir().unwrap();
    let pointer = dir.path().join("NeuralAmpModelerCore.tar.gz");
    fs::write(
        &pointer,
        "version https://git-lfs.github.com/spec/v1\noid sha256:abc\nsize 42\n",
    )
    .unwrap();
    let lock = lock(dir.path(), "v0.5.4\n");

    let err = ensure_vendor(&pointer, &lock, &dir.path().join("NeuralAmpModelerCore"))
        .expect_err("a pointer cannot be extracted");
    assert!(
        err.contains("git lfs pull"),
        "the error says how to fix it: {err}"
    );
}

#[test]
fn extracted_sources_are_newer_than_the_objects_built_before_them() {
    // The archive is reproducible, so every entry carries mtime 0. Keeping it
    // would date a NAM update in 1970 — older than the objects the last build
    // left in OUT_DIR — and the C++ build would skip recompiling it.
    let dir = tempfile::tempdir().unwrap();
    let dest = dir.path().join("NeuralAmpModelerCore");
    let before = std::time::SystemTime::now() - std::time::Duration::from_secs(60);

    ensure_vendor(
        &archive(dir.path(), "v1"),
        &lock(dir.path(), "v0.5.4\n"),
        &dest,
    )
    .unwrap();

    let modified = fs::metadata(dest.join("NAM/dsp.cpp"))
        .unwrap()
        .modified()
        .unwrap();
    assert!(
        modified > before,
        "extracted at {modified:?}, the build is newer"
    );
}

#[test]
fn a_fresh_stamp_never_makes_the_next_build_rerun() {
    // Cargo reruns a build script when a watched path is newer than the run
    // that watched it — and a rerun of this one recompiles 15 crates. The
    // stamp is written DURING the run, so it must not carry "now".
    let dir = tempfile::tempdir().unwrap();
    let dest = dir.path().join("NeuralAmpModelerCore");
    let a_day_ago = std::time::SystemTime::now() - std::time::Duration::from_secs(86_400);

    ensure_vendor(
        &archive(dir.path(), "v1"),
        &lock(dir.path(), "v0.5.4\n"),
        &dest,
    )
    .unwrap();

    let stamped = fs::metadata(dest.join(STAMP)).unwrap().modified().unwrap();
    assert!(
        stamped < a_day_ago,
        "stamp dated {stamped:?}, after the run began"
    );
}

#[test]
fn a_build_waiting_on_another_extraction_uses_the_tree_it_left() {
    // Two builds on one checkout (debug + release, RustRover + a terminal):
    // the second must wait for the first and then take its tree, never
    // delete it while the first one's compiler reads it.
    let dir = tempfile::tempdir().unwrap();
    let dest = dir.path().join("NeuralAmpModelerCore");
    let archive = archive(dir.path(), "v1");
    let lock = lock(dir.path(), "v0.5.4\n");

    let held = fs::OpenOptions::new()
        .read(true)
        .write(true)
        .create(true)
        .truncate(false)
        .open(extraction_lock(&dest))
        .unwrap();
    held.lock().unwrap();
    let (done_tx, done_rx) = std::sync::mpsc::channel();
    let (a, l, d) = (archive.clone(), lock.clone(), dest.clone());
    let waiter = std::thread::spawn(move || {
        let outcome = ensure_vendor(&a, &l, &d);
        done_tx.send(()).unwrap();
        outcome
    });
    std::thread::sleep(std::time::Duration::from_millis(300));
    assert!(
        done_rx.try_recv().is_err(),
        "the second build went ahead while the first held the tree"
    );

    // The first build finishes its tree for the same lock, then lets go.
    fs::create_dir_all(dest.join("NAM")).unwrap();
    fs::write(dest.join("NAM/dsp.cpp"), "first").unwrap();
    fs::write(dest.join(STAMP), "v0.5.4\n").unwrap();
    held.unlock().unwrap();

    assert_eq!(waiter.join().unwrap(), Ok(Outcome::Present));
    assert_eq!(
        fs::read_to_string(dest.join("NAM/dsp.cpp")).unwrap(),
        "first",
        "the tree the first build is compiling was left alone"
    );
}
