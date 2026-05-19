#!/usr/bin/env bash
# Harness-enforced: refuse to EDIT/WRITE a source file that is already
# over its line cap (Rust 600, Slint 500). Forces splitting / one
# responsibility per file BEFORE growing it — the rule I kept
# violating (local_dispatcher.rs 714). New files and under-cap files
# pass. Does not depend on me remembering.
set -euo pipefail
input="$(cat)"
f="$(printf '%s' "$input" | jq -r '.tool_input.file_path // empty')"
[ -z "$f" ] && exit 0
[ -f "$f" ] || exit 0   # new file: allowed
case "$f" in
  *_tests.rs|*test*) exit 0 ;;   # test files: not the target of this rule
  *.rs)    cap=600 ;;
  *.slint) cap=500 ;;
  *)       exit 0 ;;
esac
n="$(wc -l < "$f" | tr -d ' ')"
if [ "$n" -gt "$cap" ]; then
  jq -n --arg f "$f" --arg n "$n" --arg c "$cap" '{
    hookSpecificOutput: {
      hookEventName: "PreToolUse",
      permissionDecision: "deny",
      permissionDecisionReason: ("FILE-PER-FEATURE / CAP (projeto OpenRig): " + $f + " tem " + $n + " linhas > " + $c + ". Proibido crescer arquivo acima do cap ou que faca >1 coisa. DIVIDA primeiro (um handler/feature por arquivo, dispatcher = roteador fino) e so entao edite. docs/development/file-organization.md / openrig-code-quality.")
    }
  }'
fi
exit 0
