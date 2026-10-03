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
# carries that stamp. It runs on macOS: a failure that only exists on Linux
# (a `cfg(target_os = "linux")` path, the JACK backend) still shows up in CI
# only. PRE_PR_GATE_SUITE replaces the checks (tests only).
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
  cargo test --workspace
fi

if [ "$(git rev-parse HEAD)" != "$head" ]; then
  echo "pre-pr-gate: HEAD moved while the checks ran — run it again." >&2
  exit 1
fi
printf '%s\n' "$head" > "$stamp"
echo "pre-pr-gate: passed on $head"
