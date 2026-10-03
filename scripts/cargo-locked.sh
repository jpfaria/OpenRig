#!/usr/bin/env bash
# Responsibility: runs one cargo command under the machine-wide build lock.
#
#   scripts/cargo-locked.sh test -p engine
#   scripts/cargo-locked.sh run -p adapter-gui -- --mcp
#
# Every agent build goes through it (the `.claude/hooks/build-lock-guard.sh`
# hook denies a bare `cargo build/test/check/run/...`), so the agents on this
# machine build one at a time (scripts/build-lock.sh). `run` builds under the
# lock and starts the program after releasing it: an app left open must not
# hold every other build back. The exit code is cargo's.
set -euo pipefail

# shellcheck source=build-lock.sh
. "$(dirname "$0")/build-lock.sh"

build_lock_acquire

if [ "${1:-}" = "run" ]; then
  build_args=()
  for arg in "${@:2}"; do
    [ "$arg" = "--" ] && break
    build_args+=("$arg")
  done
  cargo build ${build_args[@]+"${build_args[@]}"}
  build_lock_release
  trap - EXIT
  exec cargo "$@"
fi

status=0
cargo "$@" || status=$?
exit "$status"
