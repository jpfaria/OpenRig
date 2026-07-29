#!/bin/bash
# Release version helpers shared by the release workflow (issue #820).
#
# The git tag is the source of truth for the released version. The workflow
# derives the version from the tag and writes it into the workspace manifest
# BEFORE building, so the compiled binary reports the same version the release
# artifacts are named after — `env!("CARGO_PKG_VERSION")` is what the launcher
# footer renders.

# True when the argument is a bare semver. Uses grep rather than the bash-only
# `[[ =~ ]]` so the helper behaves the same whichever shell sources it — zsh
# rejects this pattern outright, and a silently-empty version corrupts the
# manifest it is about to write.
_is_semver() {
    printf '%s' "${1-}" \
        | grep -Eq '^[0-9]+\.[0-9]+\.[0-9]+(-[0-9A-Za-z.-]+)?(\+[0-9A-Za-z.-]+)?$'
}

# Echo the semver carried by a git ref name ("v0.1.1" -> "0.1.1").
# Fails when the ref is not a release tag: the workflow falls back to "dev" for
# manual runs, and Cargo would reject that.
release_version_from_tag() {
    local ref="${1-}"
    local ver="${ref#v}"

    if ! _is_semver "$ver"; then
        echo "ERROR: '$ref' is not a release tag (expected vMAJOR.MINOR.PATCH)" >&2
        return 1
    fi

    printf '%s\n' "$ver"
}

# Rewrite the `version` key of [workspace.package] in a Cargo manifest.
# Scoped to that one section so the version pins in [workspace.dependencies]
# survive untouched.
set_workspace_version() {
    local manifest="${1-}"
    local version="${2-}"

    if [ ! -f "$manifest" ]; then
        echo "ERROR: manifest not found: $manifest" >&2
        return 1
    fi

    # Refuse before writing. A caller that feeds in a failed
    # release_version_from_tag would otherwise leave `version = ""` behind and
    # cargo can no longer parse the workspace at all.
    if ! _is_semver "$version"; then
        echo "ERROR: '$version' is not a semver version" >&2
        return 1
    fi

    local tmp
    tmp="$(mktemp)"

    if ! awk -v ver="$version" '
        /^[[:space:]]*\[/ { in_section = ($0 ~ /^[[:space:]]*\[workspace\.package\][[:space:]]*$/) }
        in_section && !done && /^[[:space:]]*version[[:space:]]*=/ {
            sub(/=.*/, "= \"" ver "\"")
            done = 1
        }
        { print }
        END { exit(done ? 0 : 1) }
    ' "$manifest" > "$tmp"; then
        rm -f "$tmp"
        echo "ERROR: no version key under [workspace.package] in $manifest" >&2
        return 1
    fi

    cat "$tmp" > "$manifest"
    rm -f "$tmp"
    echo "workspace version set to $version in $manifest"
}
