#!/usr/bin/env bash
# Responsibility: denies a PR or a PR-branch push that the pre-PR gate did not pass.
#
# PreToolUse hook on Bash. `gh pr create`, and `git push` from a branch whose
# PR is open, are denied unless HEAD of the repo they run in carries the stamp
# written by `scripts/pre-pr-gate.sh`. Pushing to a branch with no PR is never
# gated. The repo is the one a `cd <dir>` or `git -C <dir>` in the command
# names, else the call's cwd.
set -euo pipefail

input="$(cat)"
cmd="$(printf '%s' "$input" | jq -r '.tool_input.command // empty')"
cwd="$(printf '%s' "$input" | jq -r '.cwd // empty')"

opens_pr=false
pushes=false
printf '%s' "$cmd" | grep -Eq '(^|[;&|[:space:]])gh[[:space:]]+pr[[:space:]]+create' && opens_pr=true
printf '%s' "$cmd" | grep -Eq '(^|[;&|[:space:]])git([[:space:]]+-C[[:space:]]+[^[:space:]]+)?[[:space:]]+push([[:space:]]|$)' && pushes=true
if ! $opens_pr && ! $pushes; then
  exit 0
fi

dir="$(printf '%s' "$cmd" | sed -nE 's/.*git[[:space:]]+-C[[:space:]]+([^[:space:];&|]+).*/\1/p' | head -1)"
[ -z "$dir" ] && dir="$(printf '%s' "$cmd" | sed -nE 's/^[[:space:]]*cd[[:space:]]+([^[:space:];&|]+).*/\1/p' | head -1)"
[ -z "$dir" ] && dir="$cwd"
dir="${dir%\"}"; dir="${dir#\"}"; dir="${dir%\'}"; dir="${dir#\'}"
git -C "$dir" rev-parse --git-dir >/dev/null 2>&1 || exit 0

if ! $opens_pr; then
  branch="$(git -C "$dir" rev-parse --abbrev-ref HEAD)"
  open="$(gh pr list --head "$branch" --state open --json number --jq length 2>/dev/null || echo 0)"
  [ "${open:-0}" = "0" ] && exit 0
fi

head="$(git -C "$dir" rev-parse HEAD)"
stamp_file="$(cd "$dir" && git rev-parse --git-path openrig-pre-pr-gate)"
case "$stamp_file" in /*) ;; *) stamp_file="$dir/$stamp_file" ;; esac
# A commit in the same command lands after this check, unstamped.
commits=false
printf '%s' "$cmd" | grep -Eq '(^|[;&|[:space:]])git([[:space:]]+-C[[:space:]]+[^[:space:]]+)?[[:space:]]+commit([[:space:]]|$)' && commits=true
if ! $commits && [ -f "$stamp_file" ] && [ "$(cat "$stamp_file")" = "$head" ]; then
  exit 0
fi

jq -n --arg r "Pre-PR gate: run ./scripts/pre-pr-gate.sh in $dir on the committed HEAD first — it runs CI's suite here (fmt, static checks, cargo test --workspace) so a PR never breaks in CI. Commit, run it, then push in a command of its own." '{
  hookSpecificOutput: {
    hookEventName: "PreToolUse",
    permissionDecision: "deny",
    permissionDecisionReason: $r
  }
}'
