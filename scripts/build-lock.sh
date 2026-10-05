#!/usr/bin/env bash
# Responsibility: holds the machine-wide lock that lets one agent build run at a time.
#
# Sourced by scripts/cargo-locked.sh and scripts/pre-pr-gate.sh. Agent sessions
# build in their own clones but share one Mac; two builds at once slow both
# several times over and turn the clock-bound tests flaky. The lock is the
# directory OPENRIG_BUILD_LOCK (default /tmp/openrig-build.lock) holding the
# holder's pid; a second build waits for it, and a lock left by a process that
# died is taken over. The holder exports OPENRIG_BUILD_LOCK_HELD=1, so a build
# it starts (the gate's cargo calls) does not wait for itself.

build_lock_path() {
  printf '%s' "${OPENRIG_BUILD_LOCK:-/tmp/openrig-build.lock}"
}

build_lock_acquire() {
  [ "${OPENRIG_BUILD_LOCK_HELD:-}" = "1" ] && return 0
  local lock holder waiting=""
  lock="$(build_lock_path)"
  until mkdir "$lock" 2>/dev/null; do
    holder="$(cat "$lock/pid" 2>/dev/null || true)"
    if [ -n "$holder" ] && ! kill -0 "$holder" 2>/dev/null; then
      rm -rf "$lock"
      continue
    fi
    if [ -z "$waiting" ]; then
      echo "build-lock: another build is running (pid ${holder:-?}) — waiting for it." >&2
      waiting=1
    fi
    sleep 1
  done
  echo $$ > "$lock/pid"
  BUILD_LOCK_OWNED=1
  export OPENRIG_BUILD_LOCK_HELD=1
  trap build_lock_release EXIT
}

build_lock_release() {
  [ "${BUILD_LOCK_OWNED:-}" = "1" ] || return 0
  rm -rf "$(build_lock_path)"
  BUILD_LOCK_OWNED=""
  unset OPENRIG_BUILD_LOCK_HELD
}
