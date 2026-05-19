#!/usr/bin/env bash
# RED-FIRST guard (issue #436 lesson). Blocks investigating PRODUCTION
# Rust source before a failing test exists. The harness runs this — it
# does not depend on the model remembering the rule.
#
# Allowed always: test files (*_tests.rs, **/tests/**, names with "test"),
# .slint, docs, anything outside crates/**/src, and ALL writes/edits
# (you must be able to write the test). Production source reads/greps
# are DENIED until the unlock sentinel exists.
#
# Unlock: after you have written the failing test AND run it AND seen it
# go RED, create `.claude/.red-first-unlocked`. That act is visible in
# the transcript, so skipping the RED is auditable. Remove it when
# starting the next bug.
set -euo pipefail

input="$(cat)"
tool="$(printf '%s' "$input" | jq -r '.tool_name // empty')"
repo_root="$(cd "$(dirname "${BASH_SOURCE[0]}")/../.." && pwd)"

[ -f "$repo_root/.claude/.red-first-unlocked" ] && exit 0

# What path/command is this tool touching?
case "$tool" in
  Read)  target="$(printf '%s' "$input" | jq -r '.tool_input.file_path // empty')" ;;
  Grep|Glob)
         target="$(printf '%s' "$input" | jq -r '(.tool_input.path // "") + " " + (.tool_input.glob // "") + " " + (.tool_input.pattern // "")')" ;;
  Bash)  cmd="$(printf '%s' "$input" | jq -r '.tool_input.command // empty')"
         case "$cmd" in
           *grep*|*' sed '*|*'cat '*|*' awk '*|*' rg '*|*'head '*|*'tail '*) target="$cmd" ;;
           *) exit 0 ;;
         esac ;;
  *) exit 0 ;;
esac

# Production Rust source = crates/**/src/**/*.rs, NOT a test file.
if printf '%s' "$target" | grep -qE 'crates/[^ ]*/src/[^ ]*\.rs' \
   && ! printf '%s' "$target" | grep -qiE '_tests?\.rs|/tests?/|test'; then
  jq -n '{
    hookSpecificOutput: {
      hookEventName: "PreToolUse",
      permissionDecision: "deny",
      permissionDecisionReason: "RED-FIRST (projeto OpenRig): proibido investigar codigo de producao antes de um teste FALHAR. Escreva o teste que reproduz o bug, rode-o, veja o RED, mostre ao usuario. So entao crie .claude/.red-first-unlocked e prossiga. (CLAUDE.md / docs/testing.md / openrig-code-quality)"
    }
  }'
  exit 0
fi
exit 0
