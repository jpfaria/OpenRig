#!/usr/bin/env bash
# Responsibility: runs CI's checks locally and stamps the commit they passed on.
#
# Run it from the clone, on a committed tree, before `gh pr create` and before
# pushing to a branch whose PR is open:
#
#   ./scripts/pre-pr-gate.sh
#
# It runs what the Test workflow runs — fmt, the whole-repo static checks and
# `cargo test --workspace` — and on success records HEAD in the clone's git dir.
# `.claude/hooks/pre-pr-gate-guard.sh` denies the PR or the push unless HEAD
# carries that stamp. A warning fails it like an error. The tests run on
# macOS; the Linux build (a `cfg(target_os = "linux")` path, the JACK backend)
# is checked for errors and warnings in the Debian container of
# docker/Dockerfile.linux-builder, its target kept in a Docker volume so only
# the first run is cold. With cargo-nextest installed the tests run through it
# (every test binary in parallel) plus `cargo test --doc`, which nextest does
# not run; without it, plain `cargo test`. PRE_PR_GATE_SUITE replaces the
# checks (tests only).
set -euo pipefail

if [ -n "$(git status --porcelain --untracked-files=no)" ]; then
  echo "pre-pr-gate: commit your changes first — the stamp is for a commit." >&2
  exit 1
fi

head="$(git rev-parse HEAD)"
stamp="$(git rev-parse --git-path openrig-pre-pr-gate)"
rm -f "$stamp"

if [ -n "${PRE_PR_GATE_SUITE:-}" ]; then
  bash -c "$PRE_PR_GATE_SUITE"
else
  cargo fmt --all -- --check
  VALIDATE_STATIC_ONLY=1 ./scripts/validate.sh crates

  if ! docker info >/dev/null 2>&1; then
    echo "pre-pr-gate: Docker is not running — the Linux build cannot be checked." >&2
    exit 1
  fi
  image="openrig-pre-pr-gate-linux"
  if ! docker image inspect "$image" >/dev/null 2>&1; then
    docker build -t "$image" -f docker/Dockerfile.linux-builder docker
  fi
  docker run --rm -v "$PWD:/workspace" -v openrig-pre-pr-linux-target:/target \
    -v openrig-pre-pr-linux-registry:/root/.cargo/registry \
    -e CARGO_TARGET_DIR=/target -e RUSTFLAGS="-D warnings" -w /workspace "$image" \
    cargo check --workspace --tests

  log="$(mktemp)"
  trap 'rm -f "$log"' EXIT
  if cargo nextest --version >/dev/null 2>&1; then
    cargo nextest run --workspace --no-fail-fast 2>&1 | tee "$log"
    cargo test --workspace --doc 2>&1 | tee -a "$log"
  else
    cargo test --workspace --no-fail-fast 2>&1 | tee "$log"
  fi
  if grep -Eq '^warning: .* generated [0-9]+ warnings?' "$log"; then
    echo "pre-pr-gate: the build has warnings — fix them first." >&2
    exit 1
  fi
fi

if [ "$(git rev-parse HEAD)" != "$head" ]; then
  echo "pre-pr-gate: HEAD moved while the checks ran — run it again." >&2
  exit 1
fi
printf '%s\n' "$head" > "$stamp"
echo "pre-pr-gate: passed on $head"
