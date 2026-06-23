// Issue #726 — the build.rs translation step rewrote the `.mo` catalogs on
// every build even when the `.po` had not changed. Because build.rs also
// tracks `translations/` via `cargo:rerun-if-changed`, the rewrite bumped the
// dir mtime, Cargo declared the build script stale, and `adapter-gui` was
// recompiled + relinked (~30s) on EVERY run. The fix gates the rewrite on a
// freshness check: only (re)compile a `.mo` when it is missing or its source
// `.po` is newer. These tests pin that freshness decision.

use std::time::{Duration, SystemTime};

use adapter_gui::mo_freshness::mo_is_stale;

#[test]
fn recompiles_when_mo_absent() {
    // No compiled catalog yet → must compile.
    assert!(mo_is_stale(SystemTime::UNIX_EPOCH, None));
}

#[test]
fn recompiles_when_po_is_newer_than_mo() {
    let mo = SystemTime::UNIX_EPOCH;
    let po = mo + Duration::from_secs(10);
    // Source edited after the last compile → must recompile.
    assert!(mo_is_stale(po, Some(mo)));
}

#[test]
fn skips_when_mo_is_newer_than_po() {
    let po = SystemTime::UNIX_EPOCH;
    let mo = po + Duration::from_secs(10);
    // Already compiled and up to date → must NOT rewrite (this is the bug fix:
    // rewriting here is what bumped translations/ and self-invalidated).
    assert!(!mo_is_stale(po, Some(mo)));
}

#[test]
fn skips_when_mo_equals_po() {
    let t = SystemTime::UNIX_EPOCH + Duration::from_secs(5);
    // Equal mtime → up to date, no rewrite.
    assert!(!mo_is_stale(t, Some(t)));
}
