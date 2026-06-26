#!/usr/bin/env bash
# MAIN-FOLDER GUARD (LEI ZERO OpenRig).
#
# Agents may mutate the repo ONLY from inside a .solvers/issue-N workspace
# (an isolated clone). A Claude session rooted in the MAIN folder (repo_root
# NOT under .solvers/) is forbidden from editing repo files or running git —
# no matter what the model "remembers". The harness enforces it.
#
# This gates only Claude's tools. The user's own terminal is untouched.
# Reads/greps stay allowed so Q&A from the main folder still works; only
# mutations (Edit/Write/NotebookEdit into the repo, and git/Bash that touch
# the repo) are denied.
set -euo pipefail

input="$(cat)"
tool="$(printf '%s' "$input" | jq -r '.tool_name // empty')"
repo_root="$(cd "$(dirname "${BASH_SOURCE[0]}")/../.." && pwd)"

# Solver workspace = the only place agents may write. Allow everything.
case "$repo_root" in
  */.solvers/*) exit 0 ;;
esac

deny() {
  jq -n --arg r "$1" '{
    hookSpecificOutput: {
      hookEventName: "PreToolUse",
      permissionDecision: "deny",
      permissionDecisionReason: $r
    }
  }'
  exit 0
}

REASON="LEI ZERO (OpenRig): agents NUNCA tocam a pasta principal. Esta sessao esta na pasta principal do repo — Edit/Write/git em arquivos do repo estao BLOQUEADOS. Trabalhe SOMENTE em .solvers/issue-N (clone isolado): clone a branch la e edite la. (CLAUDE.md LEI ZERO)"

case "$tool" in
  Edit|Write|NotebookEdit)
    f="$(printf '%s' "$input" | jq -r '.tool_input.file_path // empty')"
    case "$f" in
      "$repo_root"/*|"$repo_root") deny "$REASON" ;;  # inside the main repo
      /*) exit 0 ;;                                     # absolute path elsewhere (scratchpad) — allow
      *)  deny "$REASON" ;;                             # relative path = relative to main repo
    esac ;;
  Bash)
    cmd="$(printf '%s' "$input" | jq -r '.tool_input.command // empty')"
    # any git invocation from the main folder is forbidden
    if printf '%s' "$cmd" | grep -qE '(^|[^[:alnum:]_])git([^[:alnum:]_]|$)'; then
      deny "$REASON"
    fi
    # any command that names the main repo path is forbidden
    case "$cmd" in
      *"$repo_root"*) deny "$REASON" ;;
    esac ;;
esac
exit 0
