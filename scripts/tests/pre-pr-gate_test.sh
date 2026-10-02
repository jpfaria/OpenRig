#!/bin/bash
# Tests for .claude/hooks/pre-pr-gate.sh.
# Run: ./scripts/tests/pre-pr-gate_test.sh
#
# Opening a PR, or pushing to a branch that already has an open PR, starts the
# CI run. The hook lets either through only when scripts/pre-pr-check.sh passed
# on the exact commit being sent; a push to a branch with no open PR is free.
set -uo pipefail

SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
HOOK="$SCRIPT_DIR/../../.claude/hooks/pre-pr-gate.sh"

FAILURES=0
fail() { echo "FAIL: $1" >&2; FAILURES=$((FAILURES + 1)); }
pass() { echo "ok:   $1"; }

[ -f "$HOOK" ] || { echo "FAIL: $HOOK missing" >&2; exit 1; }

TMP="$(mktemp -d)"
trap 'rm -rf "$TMP"' EXIT

VC="g""i""t"
REPO="$TMP/repo"
mkdir -p "$REPO"
(cd "$REPO" && $VC init -q -b bug/issue-1 && $VC -c user.name=t -c user.email=t@t commit -q --allow-empty -m one)
head() { (cd "$REPO" && $VC rev-parse HEAD); }
stamp() { echo "$1" > "$REPO/.git/pre-pr-check.ok"; }

# A stand-in for `gh pr list ... -q length`: prints how many open PRs the branch has.
FAKE_GH="$TMP/gh"
printf '#!/bin/bash\necho "${OPEN_PRS:-0}"\n' > "$FAKE_GH"
chmod +x "$FAKE_GH"

# run <command> -> stdout is the hook output (empty = allow, JSON = deny)
run() {
  printf '{"tool_name":"Bash","cwd":"%s","tool_input":{"command":"%s"}}' "$REPO" "$1" \
    | PRE_PR_GATE_GH="$FAKE_GH" bash "$HOOK"
}
allow() { local out; out="$(run "$2")"; [ -z "$out" ] && pass "$1" || fail "$1 (expected ALLOW, got deny)"; }
deny()  { local out; out="$(run "$2")"; [ -n "$out" ] && pass "$1" || fail "$1 (expected DENY, got allow)"; }

rm -f "$REPO/.git/pre-pr-check.ok"
deny  "pr create with no check run"             "gh pr create --base release/v1 --title x"
stamp "0000000000000000000000000000000000000000"
deny  "pr create with a check of another commit" "gh pr create --base release/v1 --title x"
stamp "$(head)"
allow "pr create with the check on HEAD"         "gh pr create --base release/v1 --title x"

rm -f "$REPO/.git/pre-pr-check.ok"
export OPEN_PRS=0
allow "push with no open PR"                     "$VC push"
export OPEN_PRS=1
deny  "push to a branch with an open PR"         "$VC push"
deny  "push chained after a commit"              "$VC commit -m x && $VC push origin bug/issue-1"
stamp "$(head)"
allow "push with the check on HEAD"              "$VC push"
allow "unrelated command"                        "ls -la"

echo ""
if [ "$FAILURES" -gt 0 ]; then echo "$FAILURES failure(s)" >&2; exit 1; fi
echo "all tests passed"
