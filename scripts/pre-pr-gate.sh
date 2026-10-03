#!/usr/bin/env bash
# Responsibility: runs CI's tests locally and stamps the commit they passed on.
#
# Run it from the clone, on a committed tree, before `gh pr create` and before
# pushing to a branch whose PR is open:
#
#   ./scripts/pre-pr-gate.sh
#
# It runs fmt and the tests of every package the branch can affect: the
# packages owning a file changed since the release branch it was cut from,
# plus every package depending on them (scripts/pre_pr_gate_packages.py).
# A change to the root manifest, the lockfile or a file outside every crate
# tests the whole workspace; a docs-only change runs no tests. PRE_PR_GATE_BASE
# names the base instead of the nearest `origin/release/*`. The static checks
# (scripts/validate.sh) are left to CI's `Static Checks` job. On success it
# records HEAD in the clone's git dir; `.claude/hooks/pre-pr-gate-guard.sh`
# denies the PR or the push unless HEAD carries that stamp.
#
# One gate runs at a time on the machine: a second one waits on the lock
# directory PRE_PR_GATE_LOCK (default /tmp/openrig-pre-pr-gate.lock) instead of
# splitting the CPU, which also turns the clock-bound tests flaky. A lock left
# by a gate that died is taken over.
#
# It runs on macOS: a failure that only exists on Linux (a
# `cfg(target_os = "linux")` path, the JACK backend) still shows up in CI only.
# With cargo-nextest installed the tests run through it plus `cargo test
# --doc`, which nextest does not run; without it, plain `cargo test`.
# PRE_PR_GATE_SUITE replaces the checks (tests only).
set -euo pipefail

if [ -n "$(git status --porcelain --untracked-files=no)" ]; then
  echo "pre-pr-gate: commit your changes first — the stamp is for a commit." >&2
  exit 1
fi

head="$(git rev-parse HEAD)"
stamp="$(git rev-parse --git-path openrig-pre-pr-gate)"
rm -f "$stamp"

lock="${PRE_PR_GATE_LOCK:-/tmp/openrig-pre-pr-gate.lock}"
waiting=""
until mkdir "$lock" 2>/dev/null; do
  holder="$(cat "$lock/pid" 2>/dev/null || true)"
  if [ -n "$holder" ] && ! kill -0 "$holder" 2>/dev/null; then
    rm -rf "$lock"
    continue
  fi
  if [ -z "$waiting" ]; then
    echo "pre-pr-gate: another gate is running (pid ${holder:-?}) — waiting for it." >&2
    waiting=1
  fi
  sleep 1
done
trap 'rm -rf "$lock"' EXIT
echo $$ > "$lock/pid"

nearest_release() {
  local ref n best="" best_n=""
  for ref in $(git for-each-ref --format='%(refname:short)' refs/remotes/origin/release/); do
    n="$(git rev-list --count "$ref..HEAD")"
    if [ -z "$best_n" ] || [ "$n" -lt "$best_n" ]; then best="$ref"; best_n="$n"; fi
  done
  echo "$best"
}

run_tests() {
  local base meta selected
  base="${PRE_PR_GATE_BASE:-$(nearest_release)}"
  meta="$(mktemp)"
  cargo metadata --format-version 1 > "$meta"
  if [ -z "$base" ]; then
    selected="ALL"
  else
    selected="$(git diff --name-only "$base...HEAD" | python3 scripts/pre_pr_gate_packages.py "$meta")"
  fi
  local tests=() docs=()
  if [ "$selected" = "ALL" ]; then
    tests=(--workspace)
    docs=(--workspace)
  else
    local p
    for p in $selected; do tests+=(-p "$p"); done
    for p in $(git diff --name-only "$base...HEAD" | python3 scripts/pre_pr_gate_packages.py "$meta" --lib-only); do
      docs+=(-p "$p")
    done
  fi
  rm -f "$meta"
  if [ ${#tests[@]} -eq 0 ]; then
    echo "pre-pr-gate: no package changed since $base — no tests to run."
    return
  fi
  echo "pre-pr-gate: testing ${tests[*]} (changes since ${base:-nothing})"
  if cargo nextest --version >/dev/null 2>&1; then
    cargo nextest run "${tests[@]}" --no-fail-fast
    if [ ${#docs[@]} -gt 0 ]; then cargo test "${docs[@]}" --doc; fi
  else
    cargo test "${tests[@]}" --no-fail-fast
  fi
}

if [ -n "${PRE_PR_GATE_SUITE:-}" ]; then
  bash -c "$PRE_PR_GATE_SUITE"
else
  cargo fmt --all -- --check
  run_tests
fi

if [ "$(git rev-parse HEAD)" != "$head" ]; then
  echo "pre-pr-gate: HEAD moved while the checks ran — run it again." >&2
  exit 1
fi
printf '%s\n' "$head" > "$stamp"
echo "pre-pr-gate: passed on $head"
