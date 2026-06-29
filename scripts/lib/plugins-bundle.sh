#!/bin/bash
# Plugin-bundling helpers shared by the packaging scripts (issue #709).
# Sourced — do not execute. Tested by scripts/tests/plugins_bundle_test.sh.

# Resolve the plugins source directory. OPENRIG_PLUGINS_DIR overrides the
# repo-root default `plugins/source`; an explicit override that is not a
# directory is fatal (a silent empty bundle would mask the typo), while the
# default is allowed to be absent (dev machines without the plugin tree).
plugins_src_dir() {
    if [ -n "${OPENRIG_PLUGINS_DIR:-}" ]; then
        if [ ! -d "$OPENRIG_PLUGINS_DIR" ]; then
            echo "FATAL: OPENRIG_PLUGINS_DIR='$OPENRIG_PLUGINS_DIR' is not a directory" >&2
            return 1
        fi
        echo "$OPENRIG_PLUGINS_DIR"
    else
        echo "plugins/source"
    fi
}

# bundle_plugins <src> <dest> [drop-pattern...]
# Stage <src> into <dest>, then drop every plugin platform/<pattern> dir —
# each LV2 plugin ships platform/{linux-*,macos-*,windows-*} binaries and
# the non-target ones are dead weight in the bundle (issue #425).
# A missing <src> is a NOTE, not an error: the app's plugin registry falls
# back to the user-writable root when nothing is bundled.
bundle_plugins() {
    local src="$1" dest="$2"
    shift 2
    if [ ! -d "$src" ]; then
        echo "    NOTE: $src not found — bundle ships without plugins"
        return 0
    fi
    cp -r "$src" "$dest"
    local dropped_dirs=0 pattern dir
    for pattern in "$@"; do
        while IFS= read -r dir; do
            rm -rf "$dir"
            dropped_dirs=$((dropped_dirs + 1))
        done < <(find "$dest" -type d -path "*/platform/$pattern" 2>/dev/null)
    done
    local plugin_count
    plugin_count=$(find "$dest" -name 'manifest.yaml' | wc -l | tr -d ' ')
    echo "    bundled plugins ($plugin_count package(s)); dropped $dropped_dirs non-target platform dirs"
}
