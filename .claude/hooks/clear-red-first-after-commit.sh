#!/usr/bin/env bash
# Clear `.red-first-unlocked` sentinels after a successful `git commit`.
#
# Runs on PostToolUse for Bash tool calls. The contract:
#
# 1. The agent (or human) writes a failing test, creates the sentinel
#    to unlock production edits, implements the fix, commits.
# 2. *This* hook runs after the commit lands and wipes the sentinel.
# 3. The next bug therefore has to write a fresh RED test before
#    `red-first-guard.sh` lets it touch production code — there is no
#    "I left the lock open" state to drift into.
#
# The script only acts on commits that look like fixes/features
# (`fix(`, `feat(`, `bugfix(` prefix or `Fix #`/`Fixes #` body),
# because chore/docs commits don't close a TDD cycle.
#
# Idempotent: missing sentinel is a no-op. Failures of the commit
# itself (non-zero exit) are ignored so a botched commit doesn't
# erase the unlock by accident.

set -euo pipefail

input="$(cat)"
tool="$(printf '%s' "$input" | jq -r '.tool_name // empty')"
[ "$tool" = "Bash" ] || exit 0

cmd="$(printf '%s' "$input" | jq -r '.tool_input.command // empty')"
case "$cmd" in
  *"git commit"*) ;;
  *) exit 0 ;;
esac

# Tool response shape varies; tolerate either "exit_code" or "exitCode"
# and treat "missing" as 0 (success — Bash tool omits the field when
# the command succeeded).
exit_code="$(printf '%s' "$input" | jq -r '
  (.tool_response.exit_code // .tool_response.exitCode // .tool_response.success // 0) | tostring
')"
case "$exit_code" in
  0|true) ;;
  *) exit 0 ;;
esac

# Heuristic: only clear for commits that close a TDD cycle. The
# commit body is the first meaningful argument after `-m`; we grep
# the full command line (HEREDOCs included) for a conventional
# fix/feat prefix.
case "$cmd" in
  *"fix("*|*"feat("*|*"bugfix("*|*"Fix #"*|*"Fixes #"*) ;;
  *) exit 0 ;;
esac

repo_root="$(cd "$(dirname "${BASH_SOURCE[0]}")/../.." && pwd)"
removed=0
for sentinel in \
    "$repo_root/.claude/.red-first-unlocked" \
    "$repo_root"/.solvers/*/.claude/.red-first-unlocked
do
    if [ -f "$sentinel" ]; then
        rm -f "$sentinel"
        removed=$((removed + 1))
    fi
done

if [ "$removed" -gt 0 ]; then
    jq -n --arg n "$removed" '{
      hookSpecificOutput: {
        hookEventName: "PostToolUse",
        additionalContext: ("RED-first guard re-armed: cleared \($n) .red-first-unlocked sentinel(s) after commit. Next production edit needs a fresh failing test.")
      }
    }'
fi
exit 0
