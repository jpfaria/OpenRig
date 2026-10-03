#!/usr/bin/env bash
# Responsibility: blocks a PR or a push to an open PR until the local CI check passed on that commit.
#
# A PR's CI is the slowest place to learn the branch does not build on Linux or
# carries a warning. `gh pr create`, and a `git push` to a branch that already
# has an open PR, both start that CI run; either goes through only when
# scripts/pre-pr-check.sh stamped the exact HEAD being sent. A push to a branch
# with no open PR is free. Commit first, run the check, then push or open the PR.
set -euo pipefail

input="$(cat)"
cmd="$(printf '%s' "$input" | jq -r '.tool_input.command // empty')"
[ -n "$cmd" ] || exit 0

gate=""
if printf '%s' "$cmd" | grep -Eq '(^|[;&|[:space:]])gh[[:space:]]+pr[[:space:]]+create'; then
  gate="opening a PR"
elif printf '%s' "$cmd" | grep -Eq '(^|[;&|[:space:]])git[[:space:]]+(-C[[:space:]]+[^[:space:]]+[[:space:]]+)?push'; then
  gate="push"
fi
[ -n "$gate" ] || exit 0

dir="$(printf '%s' "$input" | jq -r '.cwd // empty')"
[ -n "$dir" ] || dir="$PWD"
lead_cd="$(printf '%s' "$cmd" | sed -nE 's/^cd[[:space:]]+([^[:space:];&]+).*/\1/p')"
[ -z "$lead_cd" ] || dir="$lead_cd"
git_c="$(printf '%s' "$cmd" | sed -nE 's/.*git[[:space:]]+-C[[:space:]]+([^[:space:]]+).*/\1/p')"
[ -z "$git_c" ] || dir="$git_c"
cd "$dir" 2>/dev/null || exit 0
git rev-parse --git-dir >/dev/null 2>&1 || exit 0

if [ "$gate" = "push" ]; then
  branch="$(git rev-parse --abbrev-ref HEAD)"
  open="$("${PRE_PR_GATE_GH:-gh}" pr list --head "$branch" --state open --json number -q length 2>/dev/null || echo 0)"
  [ "${open:-0}" -gt 0 ] 2>/dev/null || exit 0
  gate="push to $branch, which has an open PR"
fi

stamp="$(cat "$(git rev-parse --git-dir)/pre-pr-check.ok" 2>/dev/null || true)"
[ "$stamp" = "$(git rev-parse HEAD)" ] && exit 0

jq -n --arg r "Blocked $gate: scripts/pre-pr-check.sh has not passed on HEAD $(git rev-parse --short HEAD). Commit, run ./scripts/pre-pr-check.sh until it is green, then retry. It runs what the PR's CI would fail on (fmt, static checks, the Linux build)." '{
  hookSpecificOutput: {
    hookEventName: "PreToolUse",
    permissionDecision: "deny",
    permissionDecisionReason: $r
  }
}'
