#!/usr/bin/env bash
# Unit test for scripts/lib/patch-coverage.py — the diff × lcov intersection
# that reproduces codecov/patch, and the codecov.yml ignore list it honours.
# Runs offline: a hand-written lcov + a hand-written diff, no cargo, no network.
set -uo pipefail

cd "$(dirname "$0")/../.."
ROOT="$PWD"
TMP="$(mktemp -d)"
trap 'rm -rf "$TMP"' EXIT
fail=0

check() {
  local name="$1" expected="$2" actual="$3"
  if [[ "$actual" == *"$expected"* ]]; then
    echo "ok   — $name"
  else
    echo "FAIL — $name"
    echo "       expected to contain: $expected"
    echo "       got: $actual"
    fail=1
  fi
}

cat > "$TMP/lcov.info" <<LCOV
SF:$ROOT/crates/demo/src/covered.rs
DA:10,3
DA:11,1
end_of_record
SF:$ROOT/crates/demo/src/missed.rs
DA:20,0
DA:21,0
end_of_record
SF:$ROOT/crates/adapter-gui/src/desktop_app.rs
DA:5,0
end_of_record
LCOV

# The script reads the diff from git, so drive its scoring directly with a
# stub `git diff` on PATH — the intersection is what is under test, not git.
mkdir -p "$TMP/bin"
cat > "$TMP/bin/git" <<'STUB'
#!/usr/bin/env bash
if [ "$1" = "rev-parse" ]; then
  echo "$FAKE_ROOT"
  exit 0
fi
if [ "$1" = "diff" ]; then
  cat "$FAKE_DIFF"
  exit 0
fi
exit 0
STUB
chmod +x "$TMP/bin/git"

cat > "$TMP/scored.diff" <<DIFF
+++ b/crates/demo/src/covered.rs
@@ -9,0 +10,3 @@
+++ b/crates/demo/src/missed.rs
@@ -19,0 +20,2 @@
DIFF

cat > "$TMP/ignored.diff" <<DIFF
+++ b/crates/adapter-gui/src/desktop_app.rs
@@ -4,0 +5,1 @@
DIFF

run() {
  FAKE_ROOT="$ROOT" FAKE_DIFF="$1" PATH="$TMP/bin:$PATH" \
    python3 "$ROOT/scripts/lib/patch-coverage.py" "$TMP/lcov.info" fake-base "${2:-}"
}

out="$(run "$TMP/scored.diff")"
check "counts only instrumented added lines" "2/4 lines hit" "$out"
check "reports the percentage" "50.00%" "$out"

listed="$(run "$TMP/scored.diff" --files)"
check "--files names the file that missed" "crates/demo/src/missed.rs" "$listed"
if [[ "$listed" == *"covered.rs"* ]]; then
  echo "FAIL — a fully covered file must not be listed"; fail=1
else
  echo "ok   — a fully covered file is not listed"
fi

# desktop_app.rs is on codecov.yml's ignore list: its uncovered added line must
# not drag the number down, because the PR check does not count it either.
ignored_out="$(run "$TMP/ignored.diff")"
check "an ignored file is not scored" "0/0 lines hit" "$ignored_out"

exit $fail
