#!/usr/bin/env bash
set -euo pipefail

# Add a git submodule as a dependency with a pinned commit.
#
# Usage: ./scripts/add-dep.sh <name> <repo-url> [commit-hash]
#
# Examples:
#   ./scripts/add-dep.sh dragonfly-reverb https://github.com/michaelwillis/dragonfly-reverb.git
#   ./scripts/add-dep.sh dragonfly-reverb https://github.com/michaelwillis/dragonfly-reverb.git abc123
#
# If commit-hash is omitted, pins to the current HEAD of the default branch.

if [ $# -lt 2 ]; then
    echo "Usage: $0 <name> <repo-url> [commit-hash]"
    exit 1
fi

NAME="$1"
URL="$2"
COMMIT="${3:-}"
DEST="deps/$NAME"

PROJECT_ROOT="$(cd "$(dirname "$0")/.." && pwd)"
cd "$PROJECT_ROOT"

if [ -d "$DEST" ]; then
    echo "ERROR: $DEST already exists"
    exit 1
fi

echo "Adding submodule: $NAME"
echo "  URL:  $URL"
echo "  Path: $DEST"

git submodule add "$URL" "$DEST"

if [ -n "$COMMIT" ]; then
    echo "  Pinning to: $COMMIT"
    cd "$DEST"
    git checkout "$COMMIT"
    cd "$PROJECT_ROOT"
    git add "$DEST"
fi

# Init submodule's own submodules (e.g., DPF framework)
git submodule update --init --recursive "$DEST"

PINNED_HASH=$(cd "$DEST" && git rev-parse HEAD)
echo ""
echo "Done. $NAME pinned at $PINNED_HASH"
echo ""
echo "Don't forget to commit:"
echo "  git add .gitmodules $DEST"
echo "  git commit -m 'Add $NAME dependency ($PINNED_HASH)'"
