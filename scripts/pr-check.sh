#!/bin/bash
# Responsibility: gates a PR on the whole workspace building clean before CI sees it.
#
# Catches here what CI would otherwise report: a test that no longer compiles,
# a field doubled by a merge, a warning, unformatted code, a static rule. It
# only compiles (`cargo check`): the test run stays in CI.
# Linux/JACK-only code is not compiled on macOS: the same check runs again in
# the Debian container of docker/Dockerfile.linux-builder, its target kept in a
# Docker volume so only the first run is cold.
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

docker info >/dev/null 2>&1 || {
    echo "pr-check: Docker is not running — the Linux build cannot be checked" >&2
    exit 1
}
image="openrig-pr-check-linux"
docker image inspect "$image" >/dev/null 2>&1 ||
    docker build -t "$image" -f docker/Dockerfile.linux-builder docker
docker run --rm -v "$PWD:/workspace" -v openrig-pr-check-linux-target:/target \
    -v openrig-pr-check-linux-registry:/root/.cargo/registry \
    -e CARGO_TARGET_DIR=/target -e RUSTFLAGS="-D warnings" -w /workspace "$image" \
    cargo check --workspace --lib --bins --tests --examples --message-format short
echo "pr-check: ok"
