#!/bin/bash
# Tests for the minimum font-size check in scripts/validate.sh (issue #954).
# Run: ./scripts/tests/min_font_size_test.sh
#
# Nothing in the app may render text smaller than the preset select's own text
# (18px). The check is what keeps a smaller size from coming back.
set -uo pipefail

SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
REPO_ROOT="$(cd "$SCRIPT_DIR/../.." && pwd)"
VALIDATE="$REPO_ROOT/scripts/validate.sh"

FAILURES=0
fail() { echo "FAIL: $1" >&2; FAILURES=$((FAILURES + 1)); }
pass() { echo "ok:   $1"; }

[ -f "$VALIDATE" ] || { echo "FAIL: $VALIDATE missing" >&2; exit 1; }

TMP="$(mktemp -d)"
trap 'rm -rf "$TMP"' EXIT

write_slint() { # <path> <font-size line>
  mkdir -p "$(dirname "$1")"
  cat > "$1" <<EOF
// Responsibility: renders one probe label
export component Probe inherits Rectangle {
    Text {
        text: "probe";
        $2
    }
}
EOF
}

# validate.sh resolves paths from the repo root, so the fixtures live inside it.
FIXTURES="$REPO_ROOT/.validate-font-fixtures"
rm -rf "$FIXTURES"
mkdir -p "$FIXTURES"
trap 'rm -rf "$TMP" "$FIXTURES"' EXIT

run_on() { # <file> -> validate output
  ( cd "$REPO_ROOT" && VALIDATE_STATIC_ONLY=1 ./scripts/validate.sh "$1" 2>&1 )
}

small="$FIXTURES/small.slint"
write_slint "$small" "font-size: 12px;"
out="$(run_on "${small#"$REPO_ROOT"/}")"
echo "$out" | grep -q "minimum font size" && pass "12px is rejected" || fail "12px must be rejected (got: $out)"

exact="$FIXTURES/exact.slint"
write_slint "$exact" "font-size: 18px;"
out="$(run_on "${exact#"$REPO_ROOT"/}")"
echo "$out" | grep -q "minimum font size" && fail "18px must be accepted (got: $out)" || pass "18px is accepted"

bigger="$FIXTURES/bigger.slint"
write_slint "$bigger" "font-size: 24px;"
out="$(run_on "${bigger#"$REPO_ROOT"/}")"
echo "$out" | grep -q "minimum font size" && fail "24px must be accepted (got: $out)" || pass "24px is accepted"

token="$FIXTURES/token.slint"
write_slint "$token" "font-size: Theme.min-font;"
out="$(run_on "${token#"$REPO_ROOT"/}")"
echo "$out" | grep -q "minimum font size" && fail "the token must be accepted (got: $out)" || pass "Theme.min-font is accepted"

short="$FIXTURES/short.slint"
write_slint "$short" "font-size: 18px; height: 14px;"
out="$(run_on "${short#"$REPO_ROOT"/}")"
echo "$out" | grep -q "shorter than its text" && pass "an 18px Text in a 14px box is rejected" || fail "an 18px Text in a 14px box must be rejected (got: $out)"

tall="$FIXTURES/tall.slint"
write_slint "$tall" "font-size: 18px; height: 22px;"
out="$(run_on "${tall#"$REPO_ROOT"/}")"
echo "$out" | grep -q "shorter than its text" && fail "an 18px Text in a 22px box must be accepted (got: $out)" || pass "an 18px Text in a 22px box is accepted"

# The whole app, not just the fixtures: no .slint of ours may fall below the floor.
out="$( cd "$REPO_ROOT" && VALIDATE_STATIC_ONLY=1 ./scripts/validate.sh crates/adapter-gui/ui 2>&1 )"
echo "$out" | grep -q "minimum font size" \
  && fail "the app still has text under 18px" \
  || pass "no text under 18px in the app"
echo "$out" | grep -q "shorter than its text" \
  && fail "the app still has text boxes shorter than their text" \
  || pass "no text box in the app is shorter than its text"

[ "$FAILURES" -eq 0 ] && echo "all ok" || echo "$FAILURES failure(s)"
exit "$FAILURES"
