//! Freshness check for build.rs gettext `.mo` compilation (issue #726).
//!
//! `build.rs` writes the compiled `.mo` catalogs in-source under
//! `translations/<lang>/LC_MESSAGES/` (the path the packaging scripts read)
//! while also tracking `translations/` via `cargo:rerun-if-changed`.
//! Rewriting an up-to-date `.mo` bumps the directory mtime, which makes Cargo
//! treat the build script as stale and recompile + relink `adapter-gui` on
//! every build (~30s). Gating the rewrite on this freshness check breaks the
//! self-invalidation loop while keeping the in-source `.mo` packaging needs.
//!
//! Kept as a pure function of the two mtimes so it is trivially testable
//! without touching the filesystem (see `tests/mo_freshness_idempotence.rs`);
//! `build.rs` reads the metadata and feeds the timestamps in.

use std::time::SystemTime;

/// Returns `true` when the `.mo` must be (re)compiled: either it does not
/// exist yet (`mo_mtime` is `None`), or its source `.po` is strictly newer.
/// Equal mtimes count as up to date — no rewrite — so a clean tree stops
/// touching `translations/` and Cargo stops re-running the build script.
pub fn mo_is_stale(po_mtime: SystemTime, mo_mtime: Option<SystemTime>) -> bool {
    match mo_mtime {
        None => true,
        Some(mo_mtime) => po_mtime > mo_mtime,
    }
}
