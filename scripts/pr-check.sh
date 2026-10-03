#!/bin/bash
# Responsibility: gates a PR on the whole workspace building clean before CI sees it.
#
# Catches here what CI would otherwise report: a test that no longer compiles,
# a field doubled by a merge, a warning, unformatted code, a static rule. It
# only compiles (`cargo check`): the test run stays in CI.
# Linux/JACK-only code is not compiled on macOS; that one CI still catches.
set -euo pipefail

cd "$(dirname "$0")/.."
cargo fmt --all -- --check
VALIDATE_STATIC_ONLY=1 ./scripts/validate.sh crates >/dev/null
# The targets `cargo test --workspace` builds in CI. Warnings fail too, read
# from the output: RUSTFLAGS would invalidate the build cache.
out="$(cargo check --workspace --lib --bins --tests --examples --message-format short 2>&1)" || {
    echo "$out" >&2
    exit 1
}
if grep -E '^[^ ]+: warning|^warning: ' <<<"$out" | grep -v 'generated [0-9]* warning' >&2; then
    echo "pr-check: warnings above" >&2
    exit 1
fi
echo "pr-check: ok"
