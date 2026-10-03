#!/bin/bash
# Responsibility: runs on this machine what a PR's CI would fail on, before the PR sees it.
#
#   scripts/pre-pr-check.sh
#
# Mandatory before `gh pr create` and before any push to a branch that has an
# open PR (.claude/hooks/pre-pr-gate.sh enforces it). Checks the committed HEAD:
#   1. cargo fmt --check
#   2. the whole-repo static checks (validate.sh, static only)
#   3. the Linux build of every target with warnings as errors, in Docker with
#      the Test Suite job's system packages: cfg(target_os = "linux") code and
#      tests never compile on the Mac, so this is the only place they fail early.
# Green -> stamps HEAD in .git/pre-pr-check.ok, which the hook reads. Tests are
# not run here; the PR runs them.
#
# Docker Desktop is started when it is down and quit again at the end. The
# Linux build caches its registry and target dir in Docker volumes.
set -euo pipefail

repo="$(cd "$(dirname "$0")/.." && pwd)"
cd "$repo"

if [ -n "$(git status --porcelain --untracked-files=no)" ]; then
    echo "uncommitted changes: commit first, the check stamps HEAD" >&2
    exit 1
fi
head="$(git rev-parse HEAD)"
stamp="$(git rev-parse --git-dir)/pre-pr-check.ok"
rm -f "$stamp"

echo "== cargo fmt --check"
cargo fmt --all -- --check

echo "== static checks"
VALIDATE_STATIC_ONLY=1 ./scripts/validate.sh crates

echo "== Linux build (warnings are errors)"
started_docker=0
quit_docker() {
    if [ "$started_docker" = 1 ]; then
        osascript -e 'quit app "Docker"' >/dev/null 2>&1 || echo "could not quit Docker Desktop: quit it by hand" >&2
    fi
}
trap quit_docker EXIT
if ! docker info >/dev/null 2>&1; then
    [ "$(uname -s)" = Darwin ] || { echo "the Docker daemon is not running" >&2; exit 1; }
    open -g -a Docker
    started_docker=1
    for _ in $(seq 1 90); do
        docker info >/dev/null 2>&1 && break
        sleep 2
    done
    docker info >/dev/null 2>&1 || { echo "Docker Desktop did not come up" >&2; exit 1; }
fi

build_image() {
    docker build --pull -q "$@" -t openrig-pre-pr-linux - >/dev/null <<'EOF'
FROM rust:1-bookworm
RUN apt-get update && apt-get install -y --no-install-recommends \
      libasound2-dev libudev-dev pkg-config \
      libfontconfig1-dev libseat-dev \
      libxkbcommon-dev libinput-dev libgbm-dev \
      libjack-jackd2-dev gettext \
    && rm -rf /var/lib/apt/lists/*
EOF
}
build_image
# A pull cut short leaves a cached layer with empty files (cargo: exec format
# error): rebuild it from scratch once.
if ! docker run --rm openrig-pre-pr-linux cargo --version >/dev/null 2>&1; then
    build_image --no-cache
    docker run --rm openrig-pre-pr-linux cargo --version >/dev/null
fi

docker run --rm \
    -v "$repo:/src" -w /src \
    -v openrig-pre-pr-cargo-registry:/usr/local/cargo/registry \
    -v openrig-pre-pr-target:/target \
    -e CARGO_TARGET_DIR=/target \
    -e RUSTFLAGS="-D warnings" \
    -e CARGO_TERM_COLOR=always \
    openrig-pre-pr-linux \
    cargo check --workspace --all-targets

echo "$head" > "$stamp"
echo "pre-pr check green on $(git rev-parse --short HEAD)"
