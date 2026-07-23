#!/bin/bash
# Tests for scripts/lib/release-version.sh (issue #820).
# Run: ./scripts/tests/release_version_test.sh
set -uo pipefail

SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
LIB="$SCRIPT_DIR/../lib/release-version.sh"

FAILURES=0
fail() { echo "FAIL: $1" >&2; FAILURES=$((FAILURES + 1)); }
pass() { echo "ok:   $1"; }

if [ ! -f "$LIB" ]; then
    echo "FAIL: $LIB does not exist" >&2
    exit 1
fi
# shellcheck source=../lib/release-version.sh
source "$LIB"

TMP="$(mktemp -d)"
trap 'rm -rf "$TMP"' EXIT

# A workspace manifest shaped like the real one: the release version lives under
# [workspace.package], and unrelated `version =` keys live under
# [workspace.dependencies]. Only the first must ever be rewritten.
write_fixture() {
    cat > "$1" <<'TOML'
[workspace]
resolver = "2"
members = ["crates/*"]

[workspace.package]
edition = "2021"
version = "0.1.0"
license = "MIT"

[workspace.dependencies]
serde = { version = "1", features = ["derive"] }
slint = { version = "1.16", default-features = false }
TOML
}

# ── release_version_from_tag: strips the leading v ──────────────────────────
if [ "$(release_version_from_tag v0.1.1)" = "0.1.1" ]; then
    pass "release_version_from_tag strips the leading v"
else
    fail "release_version_from_tag v0.1.1: got '$(release_version_from_tag v0.1.1)'"
fi

# ── release_version_from_tag: accepts a bare semver ─────────────────────────
if [ "$(release_version_from_tag 2.10.3)" = "2.10.3" ]; then
    pass "release_version_from_tag accepts a bare semver"
else
    fail "release_version_from_tag 2.10.3: got '$(release_version_from_tag 2.10.3)'"
fi

# ── release_version_from_tag: keeps a pre-release suffix ────────────────────
if [ "$(release_version_from_tag v0.1.0-dev.24)" = "0.1.0-dev.24" ]; then
    pass "release_version_from_tag keeps the pre-release suffix"
else
    fail "release_version_from_tag v0.1.0-dev.24: got '$(release_version_from_tag v0.1.0-dev.24)'"
fi

# ── release_version_from_tag: a non-semver ref is FATAL ─────────────────────
# The workflow falls back to "dev" for manual runs; that must never reach
# Cargo.toml, which only accepts semver.
if release_version_from_tag dev >/dev/null 2>&1; then
    fail "release_version_from_tag must reject 'dev'"
else
    pass "release_version_from_tag rejects a non-semver ref"
fi

if release_version_from_tag "" >/dev/null 2>&1; then
    fail "release_version_from_tag must reject an empty ref"
else
    pass "release_version_from_tag rejects an empty ref"
fi

# ── set_workspace_version: rewrites [workspace.package] version ─────────────
MANIFEST="$TMP/Cargo.toml"
write_fixture "$MANIFEST"
set_workspace_version "$MANIFEST" "0.1.1" >/dev/null

if grep -qx 'version = "0.1.1"' "$MANIFEST"; then
    pass "set_workspace_version rewrites the workspace version"
else
    fail "workspace version not rewritten: $(grep -n '^version' "$MANIFEST")"
fi

# ── set_workspace_version: never touches dependency versions ────────────────
if grep -q 'serde = { version = "1", features = \["derive"\] }' "$MANIFEST"; then
    pass "set_workspace_version leaves dependency versions untouched"
else
    fail "dependency version clobbered: $(grep -n 'serde' "$MANIFEST")"
fi
if grep -q 'slint = { version = "1.16"' "$MANIFEST"; then
    pass "set_workspace_version leaves slint pin untouched"
else
    fail "slint pin clobbered: $(grep -n 'slint' "$MANIFEST")"
fi

# ── set_workspace_version: leaves the rest of the section intact ────────────
if grep -qx 'edition = "2021"' "$MANIFEST" && grep -qx 'license = "MIT"' "$MANIFEST"; then
    pass "set_workspace_version leaves sibling keys intact"
else
    fail "sibling keys in [workspace.package] were altered"
fi

# ── set_workspace_version: idempotent ───────────────────────────────────────
set_workspace_version "$MANIFEST" "0.1.1" >/dev/null
if [ "$(grep -cx 'version = "0.1.1"' "$MANIFEST")" -eq 1 ]; then
    pass "set_workspace_version is idempotent"
else
    fail "re-applying duplicated the version key"
fi

# ── set_workspace_version: a non-semver version is FATAL ───────────────────
# A caller that pipes a failed release_version_from_tag straight in would
# otherwise write `version = ""` and leave the manifest unparseable by cargo.
GUARD="$TMP/guard.toml"
for BAD in "" "dev" "v0.1.1"; do
    write_fixture "$GUARD"
    if set_workspace_version "$GUARD" "$BAD" >/dev/null 2>&1; then
        fail "set_workspace_version must reject the version '$BAD'"
    elif grep -qx 'version = "0.1.0"' "$GUARD"; then
        pass "set_workspace_version rejects '$BAD' and leaves the manifest intact"
    else
        fail "set_workspace_version corrupted the manifest with '$BAD'"
    fi
done

# ── set_workspace_version: a manifest without the key is FATAL ──────────────
NOKEY="$TMP/nokey.toml"
printf '[workspace.package]\nedition = "2021"\n' > "$NOKEY"
if set_workspace_version "$NOKEY" "0.1.1" >/dev/null 2>&1; then
    fail "set_workspace_version must fail when [workspace.package] has no version"
else
    pass "set_workspace_version fails loud on a manifest without a version key"
fi

# ── set_workspace_version: a missing file is FATAL ──────────────────────────
if set_workspace_version "$TMP/does-not-exist.toml" "0.1.1" >/dev/null 2>&1; then
    fail "set_workspace_version must fail on a missing manifest"
else
    pass "set_workspace_version fails loud on a missing manifest"
fi

# ── the real workspace manifest is the one the workflow patches ─────────────
# Guards the contract between this helper and .github/workflows/release.yml:
# if the real manifest ever stops carrying the key, the workflow would bump
# nothing and ship a stale version again — exactly the #820 regression.
REAL="$SCRIPT_DIR/../../Cargo.toml"
REAL_COPY="$TMP/real-Cargo.toml"
cp "$REAL" "$REAL_COPY"
if set_workspace_version "$REAL_COPY" "9.9.9" >/dev/null 2>&1 \
    && grep -qx 'version = "9.9.9"' "$REAL_COPY"; then
    pass "set_workspace_version patches the real workspace manifest"
else
    fail "set_workspace_version cannot patch the real Cargo.toml"
fi

echo ""
if [ "$FAILURES" -gt 0 ]; then
    echo "$FAILURES failure(s)" >&2
    exit 1
fi
echo "all tests passed"
