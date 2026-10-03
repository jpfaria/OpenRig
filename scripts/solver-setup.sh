#!/bin/bash
# Responsibility: builds a ready-to-run agent workspace at .solvers/issue-N.
#
# One command instead of four steps agents kept forgetting: a real clone (never
# a worktree), the NAM sources the build needs (the LFS archive a clone already
# carries since #974; the submodule on older branches), the `plugins` link to the
# owner's plugins checkout (without it the app opens with zero plugins, #938),
# and the exact absolute command to run the app from the workspace.
#
#   scripts/solver-setup.sh <issue-number> <branch> [base-branch]
#
# <branch> exists on origin -> it is checked out; otherwise it is created from
# [base-branch] (required then: the active release/vX.Y.Z) and pushed.
# Safe to re-run: an existing workspace is only completed (submodule + link).
#
# OPENRIG_SOLVERS_ROOT set (the machine owner names it in his global CLAUDE.md,
# e.g. a folder on an external disk) -> the clone, and so its target/, lives in
# $OPENRIG_SOLVERS_ROOT/issue-N and .solvers/issue-N is a link to it. Unset ->
# the clone lives in .solvers/issue-N itself.
set -euo pipefail

issue="${1:?usage: solver-setup.sh <issue-number> <branch> [base-branch]}"
branch="${2:?usage: solver-setup.sh <issue-number> <branch> [base-branch]}"
base="${3:-}"

repo_root="$(cd "$(dirname "$0")/.." && pwd)"
# Invoked from inside a solver: climb back to the main checkout.
case "$repo_root" in
    */.solvers/*) repo_root="${repo_root%%/.solvers/*}" ;;
esac
ws="$repo_root/.solvers/issue-$issue"
clone_dir="$ws"
if [ -n "${OPENRIG_SOLVERS_ROOT:-}" ] && [ ! -e "$ws" ]; then
    [ -d "$OPENRIG_SOLVERS_ROOT" ] || { echo "OPENRIG_SOLVERS_ROOT=$OPENRIG_SOLVERS_ROOT does not exist (disk not mounted?)" >&2; exit 1; }
    clone_dir="$OPENRIG_SOLVERS_ROOT/issue-$issue"
fi
remote="$(git -C "$repo_root" config --get remote.origin.url)"

if [ ! -d "$ws/.git" ]; then
    if git ls-remote --exit-code --heads "$remote" "$branch" >/dev/null; then
        git clone -q --branch "$branch" "$remote" "$clone_dir"
    else
        [ -n "$base" ] || { echo "branch $branch not on origin: pass the base release branch" >&2; exit 1; }
        git clone -q --branch "$base" "$remote" "$clone_dir"
        git -C "$clone_dir" checkout -q -b "$branch"
        git -C "$clone_dir" push -q -u origin "$branch"
    fi
    if [ "$clone_dir" != "$ws" ]; then
        mkdir -p "$repo_root/.solvers"
        ln -s "$clone_dir" "$ws"
    fi
fi
[ -d "$ws/.git" ] || { echo "$ws/.git is not a directory: not a real clone" >&2; exit 1; }

# No-op once the branch has the vendored archive (#974); older branches still
# carry the NAM submodule.
git -C "$ws" submodule update --init --recursive -q

case "$(uname -s)" in
    Darwin) config="$HOME/Library/Application Support/OpenRig/config.yaml" ;;
    *) config="${XDG_DATA_HOME:-$HOME/.local/share}/openrig/config.yaml" ;;
esac
plugins_path="$(grep -m1 'plugins_path:' "$config" 2>/dev/null | sed 's/.*plugins_path:[[:space:]]*//; s/["'\'']//g')"
if [ -z "$plugins_path" ] || [ ! -d "$plugins_path" ]; then
    echo "no usable paths.plugins_path in $config — the app would open with zero plugins" >&2
    exit 1
fi
if [ ! -e "$ws/plugins" ]; then
    ln -s "$plugins_path" "$ws/plugins"
fi
grep -qx '/plugins' "$ws/.git/info/exclude" || echo '/plugins' >> "$ws/.git/info/exclude"
# A push to a branch with an open PR runs scripts/pr-check.sh first.
git -C "$ws" config core.hooksPath scripts/hooks

echo "workspace: $ws ($branch)"
[ -L "$ws" ] && echo "lives in:  $(readlink "$ws")"
echo "plugins:   $(readlink "$ws/plugins")"
# OPENRIG_PLUGINS_ROOT outranks every config lookup (plugin-loader/src/config.rs),
# so the handed-off command loads the plugins whatever config the app reads.
echo "run:       cd $ws && OPENRIG_PLUGINS_ROOT=$plugins_path cargo run -p adapter-gui -- --mcp"
