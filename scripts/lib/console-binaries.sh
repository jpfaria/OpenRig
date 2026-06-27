#!/bin/bash
# Console-binary helpers shared by the packaging scripts (issue #741).
# Sourced — do not execute. Tested by scripts/tests/console_bundle_test.sh.
#
# The packagers used to build/stage only the GUI binary, so an installed
# OpenRig could not run headless and had no offline-render engine. These
# helpers read scripts/lib/console-binaries.tsv (the single source of truth)
# so the console + render binaries are built and staged on every platform.

# Resolve the lib's own directory ONCE at source time — BASH_SOURCE is
# reliable here, but inside a function called from another script it points
# at the caller, not this file.
_CONSOLE_BINARIES_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"

# Emit the SOT rows with comments/blank lines stripped.
_console_binaries_rows() {
    grep -vE '^[[:space:]]*(#|$)' "$_CONSOLE_BINARIES_DIR/console-binaries.tsv"
}

# console_binaries -> one "<built_basename> <installed_basename>" per line.
console_binaries() {
    _console_binaries_rows | awk '{print $2, $3}'
}

# console_build_flags -> "-p <pkg> -p <pkg> ..." for a single cargo invocation.
console_build_flags() {
    _console_binaries_rows | awk '{printf "-p %s ", $1}'
}

# stage_console_binaries <release_dir> <dest_dir> [ext]
# Copy each built binary from <release_dir> to <dest_dir> under its installed
# name, appending the optional extension (e.g. ".exe"). A missing built binary
# is FATAL — shipping a GUI-only package silently is exactly the bug #741 fixes.
stage_console_binaries() {
    local release_dir="$1" dest_dir="$2" ext="${3:-}"
    local built installed src dst
    while read -r built installed; do
        src="$release_dir/${built}${ext}"
        dst="$dest_dir/${installed}${ext}"
        if [ ! -f "$src" ]; then
            echo "FATAL: console binary $src not found — did cargo build run?" >&2
            return 1
        fi
        cp "$src" "$dst"
        chmod +x "$dst" 2>/dev/null || true
        echo "    staged ${built}${ext} -> ${installed}${ext}"
    done < <(console_binaries)
}
