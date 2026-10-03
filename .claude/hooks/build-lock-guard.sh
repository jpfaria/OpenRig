#!/usr/bin/env bash
# Responsibility: denies a cargo build that bypasses the machine-wide build lock.
#
# PreToolUse hook on Bash. A `cargo build|test|check|run|nextest|clippy|doc|
# bench` typed straight into a command is denied: it must go through
# scripts/cargo-locked.sh, which waits for any other agent's build on this
# machine first. `cargo fmt`, `cargo metadata` and the like build nothing and
# pass; so do the scripts that take the lock themselves (pre-pr-gate.sh).
set -euo pipefail

cmd="$(jq -r '.tool_input.command // empty')"

# Only where a command starts (after `;`, `&&`, `|`, `(`), past env assignments
# and wrappers like `timeout 600`: a commit message saying "cargo test" passes.
lead='(^|[;&|(])[[:space:]]*'
prefix="([A-Za-z_][A-Za-z0-9_]*=(\"[^\"]*\"|'[^']*'|[^[:space:]]*)[[:space:]]+|(timeout|nice|time|env)([[:space:]]+-?[^[:space:]-][^[:space:]]*)?[[:space:]]+)*"
build='cargo([[:space:]]+\+[^[:space:]]+)?[[:space:]]+(build|test|check|run|nextest|clippy|doc|bench|b|t|c|r)([[:space:]]|$)'
printf '%s' "$cmd" | grep -Eq "$lead$prefix$build" || exit 0

jq -n '{
  hookSpecificOutput: {
    hookEventName: "PreToolUse",
    permissionDecision: "deny",
    permissionDecisionReason: "Build lock: run cargo through scripts/cargo-locked.sh (same arguments), so builds on this machine run one at a time — e.g. scripts/cargo-locked.sh test -p engine."
  }
}'
