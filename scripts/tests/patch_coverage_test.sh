#!/usr/bin/env bash
# Unit test for scripts/lib/patch_coverage.py — the diff × lcov intersection
# that reproduces codecov/patch. Runs offline: a hand-written lcov + a
# hand-written diff, no cargo, no network.
set -uo pipefail

cd "$(dirname "$0")/../.."
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
SF:$PWD/crates/demo/src/covered.rs
DA:10,3
DA:11,1
end_of_record
SF:$PWD/crates/demo/src/missed.rs
DA:20,0
DA:21,0
end_of_record
SF:$PWD/crates/adapter-gui/src/desktop_app.rs
DA:5,0
end_of_record
LCOV

# Added lines 10-11 are instrumented and hit; the comment on 12 is not
# instrumented at all, so it must not count either way.
diff_all() {
  cat <<DIFF
--- a/crates/demo/src/covered.rs
+++ b/crates/demo/src/covered.rs
@@ -9,0 +10,3 @@
+let a = 1;
+let b = 2;
+// a comment carries no instrumentation
--- a/crates/demo/src/missed.rs
+++ b/crates/demo/src/missed.rs
@@ -19,0 +20,2 @@
+let c = 3;
+let d = 4;
DIFF
}

out="$(diff_all | PATCH_TARGET=50 python3 scripts/lib/patch_coverage.py "$TMP/lcov.info")"
check "counts only instrumented added lines" "2/4 added lines" "$out"
check "names the file that missed" "crates/demo/src/missed.rs" "$out"
if [[ "$out" == *"covered.rs"* ]]; then
  echo "FAIL — a fully covered file must not be listed"; fail=1
else
  echo "ok   — a fully covered file is not listed"
fi

diff_all | PATCH_TARGET=90 python3 scripts/lib/patch_coverage.py "$TMP/lcov.info" >/dev/null
[[ $? -eq 1 ]] && echo "ok   — below target exits 1" || { echo "FAIL — below target must exit 1"; fail=1; }

diff_all | PATCH_TARGET=10 python3 scripts/lib/patch_coverage.py "$TMP/lcov.info" >/dev/null
[[ $? -eq 0 ]] && echo "ok   — at or above target exits 0" || { echo "FAIL — above target must exit 0"; fail=1; }

# An ignored file's added lines are dropped before scoring — desktop_app.rs is
# on codecov.yml's list, so a 0-hit line there must not lower the number.
ignored_out="$(cat <<DIFF | python3 scripts/lib/patch_coverage.py "$TMP/lcov.info"
--- a/crates/adapter-gui/src/desktop_app.rs
+++ b/crates/adapter-gui/src/desktop_app.rs
@@ -4,0 +5,1 @@
+window.on_thing(|| {});
DIFF
)"
check "an ignored file is not scored" "nothing to score" "$ignored_out"

exit $fail
