#!/usr/bin/env bash
# PostToolUse(Bash): after a `git push` while on an issue branch
# (feature/issue-N or bugfix/issue-N), remind to comment the issue per gitflow
# (hash + files + build/test). Reminder ONLY — never blocks. Silent for any push
# that is not on an issue branch, so non-issue work is unaffected.
set -euo pipefail

input="$(cat)"
cmd="$(printf '%s' "$input" | jq -r '.tool_input.command // ""' 2>/dev/null || true)"

# Only react to a real `git push` (also matches `X=y git push`, `git push -u …`,
# `git push --no-verify`, `… && git push`).
printf '%s' "$cmd" | grep -qE '(^|[[:space:]&|])git[[:space:]]+push' || exit 0

branch="$(git rev-parse --abbrev-ref HEAD 2>/dev/null || true)"
issue="$(printf '%s' "$branch" | grep -oE 'issue-[0-9]+' | grep -oE '[0-9]+' | head -1 || true)"

# Not an issue branch → nothing to comment. Stay silent.
[ -z "$issue" ] && exit 0

jq -n --arg i "$issue" '{
  hookSpecificOutput: {
    hookEventName: "PostToolUse",
    additionalContext: ("Pushed on issue branch — per gitflow, comment issue #" + $i + " now: run `gh issue comment " + $i + "` with the commit hash(es), the files changed, and the build/test result.")
  }
}'
