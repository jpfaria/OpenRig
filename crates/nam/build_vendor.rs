//! Responsibility: materializes the vendored NeuralAmpModelerCore tree from its archive.
//!
//! Issue #974. The native amp modeler used to be a git submodule whose own
//! submodule (Eigen) lives on GitLab; when GitLab was down, every CI job died
//! at checkout before compiling anything. The whole tree — NAM, AudioDSPTools,
//! both Eigen copies, nlohmann — is now one archive in Git LFS
//! (`deps/NeuralAmpModelerCore.tar.gz`), pinned by a text lock
//! (`deps/NeuralAmpModelerCore.lock`). The build extracts it into
//! `deps/NeuralAmpModelerCore/` (not versioned) whenever that folder is
//! missing or was extracted from another lock, and never touches the network
//! to build.
//!
//! Shared by `build.rs` and the crate's tests (`#[path]`), so it only uses std
//! plus the `flate2` / `tar` crates both sides depend on.

use std::fs;
use std::path::{Path, PathBuf};

/// The file inside the extracted tree that records which lock it came from.
pub const STAMP: &str = ".vendor-lock";

/// What `ensure_vendor` did.
#[derive(Debug, PartialEq, Eq)]
pub enum Outcome {
    /// The tree already matched the lock; nothing was touched.
    Present,
    /// The tree was (re)extracted from the archive.
    Extracted,
}

/// Is `archive` an unmaterialized Git LFS pointer instead of the real file?
pub fn is_lfs_pointer(archive: &Path) -> bool {
    let mut head = [0u8; 40];
    let Ok(mut file) = fs::File::open(archive) else {
        return false;
    };
    use std::io::Read;
    let n = file.read(&mut head).unwrap_or(0);
    head[..n].starts_with(b"version https://git-lfs")
}

/// Make `dest` hold exactly the tree `archive` carries for `lock`.
///
/// Nothing happens when `dest` was already extracted from the same lock. An
/// archive that is still an LFS pointer (a clone without `git lfs pull`) is
/// fetched once with `git lfs pull`; if that fails the error names the fix.
pub fn ensure_vendor(archive: &Path, lock: &Path, dest: &Path) -> Result<Outcome, String> {
    let wanted =
        fs::read_to_string(lock).map_err(|e| format!("cannot read {}: {e}", lock.display()))?;
    if fs::read_to_string(dest.join(STAMP)).is_ok_and(|have| have == wanted) {
        return Ok(Outcome::Present);
    }
    if is_lfs_pointer(archive) {
        fetch_lfs_object(archive);
    }
    if is_lfs_pointer(archive) {
        return Err(format!(
            "{} is a Git LFS pointer, not the archive — run `git lfs pull` in the \
             repository (or clone it with Git LFS installed)",
            archive.display()
        ));
    }
    // Unpack beside `dest` and swap it in, so a second build racing this one
    // never compiles a half-written tree.
    let (Some(parent), Some(name)) = (dest.parent(), dest.file_name()) else {
        return Err(format!("{} has no parent folder", dest.display()));
    };
    let staging = parent.join(format!(
        ".{}.extract-{}",
        name.to_string_lossy(),
        std::process::id()
    ));
    let _ = fs::remove_dir_all(&staging);
    let unpacked = unpack(archive, &staging, name).and_then(|tree| {
        fs::write(tree.join(STAMP), &wanted)
            .map_err(|e| format!("cannot write {}: {e}", tree.join(STAMP).display()))?;
        if dest.exists() {
            fs::remove_dir_all(dest)
                .map_err(|e| format!("cannot clear {}: {e}", dest.display()))?;
        }
        fs::rename(&tree, dest).map_err(|e| format!("cannot move into {}: {e}", dest.display()))
    });
    let _ = fs::remove_dir_all(&staging);
    unpacked?;
    Ok(Outcome::Extracted)
}

/// Unpack `archive` into `staging` and return its top folder `name`.
fn unpack(archive: &Path, staging: &Path, name: &std::ffi::OsStr) -> Result<PathBuf, String> {
    let file =
        fs::File::open(archive).map_err(|e| format!("cannot open {}: {e}", archive.display()))?;
    let mut tar = tar::Archive::new(flate2::read::GzDecoder::new(file));
    // The archive's entries all say mtime 0; stamping them "now" is what makes
    // the C++ build recompile a new version.
    tar.set_preserve_mtime(false);
    tar.unpack(staging)
        .map_err(|e| format!("cannot extract {}: {e}", archive.display()))?;
    let tree = staging.join(name);
    if !tree.is_dir() {
        return Err(format!(
            "{} did not contain {}",
            archive.display(),
            name.to_string_lossy()
        ));
    }
    Ok(tree)
}

/// The repository whose LFS store holds the archive, for a checkout whose own
/// origin has none — a cargo git dependency's checkout, whose origin is cargo's
/// bare local clone. Passed as the remote (not as `lfs.url`) so a user's
/// `url.<ssh>.insteadOf` rewrite still yields a valid endpoint.
const LFS_REPOSITORY: &str = "https://github.com/jpfaria/OpenRig.git";

/// Best effort: materialize the LFS object behind `archive` (a clone made
/// without LFS). Any failure is left to the pointer check that follows.
fn fetch_lfs_object(archive: &Path) {
    let (Some(dir), Some(name)) = (archive.parent(), archive.file_name()) else {
        return;
    };
    let pull = |remote: Option<&str>| {
        let mut git = std::process::Command::new("git");
        git.arg("-C").arg(dir).args(["lfs", "pull"]);
        if let Some(remote) = remote {
            git.arg(remote);
        }
        let _ = git
            .arg("--include")
            .arg(name)
            .stdout(std::process::Stdio::null())
            .stderr(std::process::Stdio::null())
            .status();
    };
    pull(None);
    if is_lfs_pointer(archive) {
        pull(Some(LFS_REPOSITORY));
    }
}

#[cfg(test)]
#[path = "src/build_vendor_tests.rs"]
mod tests;
