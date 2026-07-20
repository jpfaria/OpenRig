#!/bin/bash
# Tests for .claude/hooks/main-folder-guard.sh (issue #751).
# Run: ./scripts/tests/main-folder-guard_test.sh
#
# The guard must protect the MAIN working tree while letting an agent work freely
# inside .solvers/. These tests pin the boundary, including the two false-positives
# fixed in #751: the OpenRig-plugins sibling repo, and bare VCS from a .solvers cwd.
set -uo pipefail

SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
HOOK_SRC="$SCRIPT_DIR/../../.claude/hooks/main-folder-guard.sh"

FAILURES=0
fail() { echo "FAIL: $1" >&2; FAILURES=$((FAILURES + 1)); }
pass() { echo "ok:   $1"; }

[ -f "$HOOK_SRC" ] || { echo "FAIL: $HOOK_SRC missing" >&2; exit 1; }

TMP="$(mktemp -d)"
trap 'rm -rf "$TMP"' EXIT

# A fake MAIN checkout (repo_root NOT under .solvers) with the hook installed,
# plus a sibling "<main>-plugins" repo to exercise the prefix-collision case.
MAIN="$TMP/OpenRig"
mkdir -p "$MAIN/.claude/hooks" "$MAIN/.solvers/issue-1/crates" "$MAIN/crates" "$TMP/OpenRig-plugins/plugins/source"
cp "$HOOK_SRC" "$MAIN/.claude/hooks/main-folder-guard.sh"
HOOK="$MAIN/.claude/hooks/main-folder-guard.sh"
VC="g""i""t"   # the version-control word, split so this runner's own command stays clean

# run <json> <cwd> -> stdout is the hook output (empty = allow, JSON = deny)
run() { printf '%s' "$1" | ( cd "$2" && bash "$HOOK" ); }
# assert ALLOW (empty) / DENY (non-empty)
allow() { local out; out="$(run "$2" "$3")"; [ -z "$out" ] && pass "$1" || fail "$1 (expected ALLOW, got deny)"; }
deny()  { local out; out="$(run "$2" "$3")"; [ -n "$out" ] && pass "$1" || fail "$1 (expected DENY, got allow)"; }

allow "Edit into .solvers"            "{\"tool_name\":\"Edit\",\"tool_input\":{\"file_path\":\"$MAIN/.solvers/issue-1/x.rs\"}}" "$MAIN"
deny  "Edit into main proper"         "{\"tool_name\":\"Edit\",\"tool_input\":{\"file_path\":\"$MAIN/crates/x.rs\"}}"           "$MAIN"
allow "Write to scratchpad"           "{\"tool_name\":\"Write\",\"tool_input\":{\"file_path\":\"/private/tmp/foo\"}}"           "$MAIN"
allow "VCS from .solvers cwd"         "{\"tool_name\":\"Bash\",\"tool_input\":{\"command\":\"$VC commit -m x\"}}"               "$MAIN/.solvers/issue-1"
deny  "VCS from main cwd"             "{\"tool_name\":\"Bash\",\"tool_input\":{\"command\":\"$VC status\"}}"                    "$MAIN"
allow "clone into .solvers (literal)" "{\"tool_name\":\"Bash\",\"tool_input\":{\"command\":\"gh repo clone a/b $MAIN/.solvers/issue-2\"}}" "$MAIN"
deny  "rm touching main by path"      "{\"tool_name\":\"Bash\",\"tool_input\":{\"command\":\"rm -rf $MAIN/crates\"}}"          "$MAIN"
allow "harmless ls from main cwd"     "{\"tool_name\":\"Bash\",\"tool_input\":{\"command\":\"ls -la\"}}"                       "$MAIN"
allow "Bash references sibling -plugins" "{\"tool_name\":\"Bash\",\"tool_input\":{\"command\":\"ls $TMP/OpenRig-plugins/plugins/source\"}}" "$MAIN"
allow "Edit into sibling -plugins"    "{\"tool_name\":\"Edit\",\"tool_input\":{\"file_path\":\"$TMP/OpenRig-plugins/x.yaml\"}}" "$MAIN"
deny  "cd into main then mutate"      "{\"tool_name\":\"Bash\",\"tool_input\":{\"command\":\"cd $MAIN && rm x\"}}"             "$MAIN"
# worktree is FORBIDDEN everywhere: it shares the parent .git and locks the branch,
# so the user's `git checkout <branch>` in the main folder aborts. Isolation is
# clone-only. The .solvers/ target must NOT buy it an exemption (issue #804).
deny  "worktree add targeting .solvers"  "{\"tool_name\":\"Bash\",\"tool_input\":{\"command\":\"$VC worktree add -b bugfix/x $MAIN/.solvers/issue-3 origin/develop\"}}" "$MAIN"
deny  "worktree add from .solvers cwd"    "{\"tool_name\":\"Bash\",\"tool_input\":{\"command\":\"$VC worktree add ../issue-9 develop\"}}"                              "$MAIN/.solvers/issue-1"
# The word "worktree" inside a commit message is NOT a worktree command.
allow "commit msg mentioning worktree"    "{\"tool_name\":\"Bash\",\"tool_input\":{\"command\":\"$VC commit -m 'drop worktree note'\"}}"                                "$MAIN/.solvers/issue-1"

echo ""
if [ "$FAILURES" -gt 0 ]; then echo "$FAILURES failure(s)" >&2; exit 1; fi
echo "all tests passed"
