#!/bin/bash
# Tests for scripts/hooks/pre-push: a push to a branch with an open PR must
# pass scripts/pr-check.sh first; a push to a branch with no PR goes through.
# Run: ./scripts/tests/pre_push_hook_test.sh
set -uo pipefail

SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
HOOK="$SCRIPT_DIR/../hooks/pre-push"

FAILURES=0
fail() { echo "FAIL: $1" >&2; FAILURES=$((FAILURES + 1)); }
pass() { echo "ok:   $1"; }

[ -x "$HOOK" ] || { echo "FAIL: $HOOK missing or not executable" >&2; exit 1; }

TMP="$(mktemp -d)"
trap 'rm -rf "$TMP"' EXIT

# Fake gh: prints how many open PRs the branch has (FAKE_PRS), or fails.
cat > "$TMP/gh" <<'EOF'
#!/bin/bash
[ "${FAKE_GH_FAIL:-0}" = 1 ] && exit 1
echo "${FAKE_PRS:-0}"
EOF
# Fake check: records that it ran, exits FAKE_CHECK_EXIT.
cat > "$TMP/check" <<EOF
#!/bin/bash
touch "$TMP/check-ran"
exit \${FAKE_CHECK_EXIT:-0}
EOF
chmod +x "$TMP/gh" "$TMP/check"

push_line="refs/heads/feature/issue-1 abc123 refs/heads/feature/issue-1 def456"
delete_line="(delete) 0000000000000000000000000000000000000000 refs/heads/feature/issue-1 def456"

# run <stdin> -> exit code of the hook; sets RAN=1 when the check ran
run() {
    rm -f "$TMP/check-ran"
    printf '%s\n' "$1" | GH="$TMP/gh" PR_CHECK="$TMP/check" bash "$HOOK" origin url >/dev/null 2>&1
    local code=$?
    RAN=0; [ -f "$TMP/check-ran" ] && RAN=1
    return $code
}

FAKE_PRS=0 run "$push_line"; code=$?
[ $code = 0 ] && [ $RAN = 0 ] && pass "no open PR: push passes without the check" || fail "no open PR (code=$code ran=$RAN)"

export FAKE_PRS=1
FAKE_CHECK_EXIT=0 run "$push_line"; code=$?
[ $code = 0 ] && [ $RAN = 1 ] && pass "open PR, check passes: push goes" || fail "open PR + green (code=$code ran=$RAN)"

FAKE_CHECK_EXIT=1 run "$push_line"; code=$?
[ $code != 0 ] && [ $RAN = 1 ] && pass "open PR, check fails: push blocked" || fail "open PR + red (code=$code ran=$RAN)"

FAKE_CHECK_EXIT=1 run "$delete_line"; code=$?
[ $code = 0 ] && [ $RAN = 0 ] && pass "branch delete: no check" || fail "delete (code=$code ran=$RAN)"
unset FAKE_PRS

FAKE_GH_FAIL=1 FAKE_CHECK_EXIT=1 run "$push_line"; code=$?
[ $code != 0 ] && [ $RAN = 1 ] && pass "gh unavailable: check runs anyway" || fail "gh down (code=$code ran=$RAN)"

[ $FAILURES = 0 ] && echo "all passed" || { echo "$FAILURES failure(s)" >&2; exit 1; }
