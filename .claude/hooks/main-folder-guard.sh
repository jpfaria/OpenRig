#!/usr/bin/env bash
# MAIN-FOLDER GUARD (LEI ZERO OpenRig).
#
# Agents may mutate the repo ONLY inside a .solvers/issue-N workspace (an
# isolated clone). The MAIN folder proper (everything under repo_root that is
# NOT under repo_root/.solvers/) is off-limits to agent Edit/Write and to git.
#
# This gates only Claude's tools. The user's own terminal is untouched.
# Reads/greps stay allowed so Q&A from the main folder still works.
#
# NOTE: the guard keys off WHAT THE OPERATION TARGETS, not where the hook file
# lives. A session rooted in the main folder is still allowed to set up and work
# inside .solvers/issue-N (clone the branch, edit, commit, push) — only writes
# that land in the main folder proper, or bare VCS commands that would run in the
# main folder's working tree, are denied.
set -euo pipefail

input="$(cat)"
tool="$(printf '%s' "$input" | jq -r '.tool_name // empty')"
repo_root="$(cd "$(dirname "${BASH_SOURCE[0]}")/../.." && pwd)"

# Hook itself running from inside a clone: that whole tree is writable. Allow.
case "$repo_root" in
  */.solvers/*) exit 0 ;;
esac

solvers="$repo_root/.solvers"

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

REASON="LEI ZERO (OpenRig): the MAIN folder is off-limits to agents. Edit/Write/VCS that land in the main working tree are BLOCKED. Work ONLY inside .solvers/issue-N (isolated clone) — set it up there, edit there, commit there, push there. (CLAUDE.md LEI ZERO)"

# True if $1 references repo_root at a PATH BOUNDARY (followed by /, end, space
# or quote). Anchored so a sibling repo like "<repo_root>-plugins" is NOT taken
# for the repo itself (issue #751).
names_main() {
  case "$1" in
    *"$repo_root"/*|*"$repo_root") return 0 ;;
    *"$repo_root"" "*|*"$repo_root"\"*|*"$repo_root"\'*) return 0 ;;
    *) return 1 ;;
  esac
}

case "$tool" in
  Edit|Write|NotebookEdit)
    f="$(printf '%s' "$input" | jq -r '.tool_input.file_path // empty')"
    case "$f" in
      "$solvers"/*)                exit 0 ;;        # inside an isolated clone — allow
      "$repo_root"/*|"$repo_root") deny "$REASON" ;; # main repo proper — block
      /*)                          exit 0 ;;        # absolute path elsewhere (scratchpad) — allow
      *)                           deny "$REASON" ;; # relative path = main repo — block
    esac ;;
  Bash)
    cmd="$(printf '%s' "$input" | jq -r '.tool_input.command // empty')"
    # Working dir already inside an isolated clone → allow (covers bare VCS like
    # `git commit` run after a cd, which carries no .solvers/ token; issue #751).
    case "$PWD" in
      "$solvers"/*|"$solvers") exit 0 ;;
    esac
    # Commands scoped to an isolated clone (.solvers/…) are allowed — VCS
    # included — as long as they do not ALSO reference the main repo OUTSIDE
    # of .solvers/. Strip every .solvers/ path token, then see what's left.
    if printf '%s' "$cmd" | grep -q '\.solvers/'; then
      stripped="$(printf '%s' "$cmd" \
        | sed "s#${solvers}/[^[:space:]]*##g" \
        | sed "s#\.solvers/[^[:space:]]*##g")"
      if names_main "$stripped"; then
        deny "$REASON"   # still names the main repo outside .solvers
      else
        exit 0           # only the clone is touched — allow
      fi
    fi
    # No .solvers/ reference → strict: no bare VCS, no main-repo path from the
    # main folder's working tree.
    if printf '%s' "$cmd" | grep -qE '(^|[^[:alnum:]_])git([^[:alnum:]_]|$)'; then
      deny "$REASON"
    fi
    names_main "$cmd" && deny "$REASON" ;;
esac
exit 0
