#!/usr/bin/env bash
# Build an arm64 .deb package locally using Docker.
#
# Convenience wrapper around build-linux-local.sh for the Orange Pi deploy
# workflow — produces only the .deb for arm64.
#
# Usage:
#   ./scripts/build-deb-local.sh [--version V]
#
# Output:
#   dist/openrig_{VERSION}_arm64.deb

set -euo pipefail

PROJECT_ROOT="$(cd "$(dirname "$0")/.." && pwd)"
VERSION="0.0.0-dev"

while [ $# -gt 0 ]; do
    case "$1" in
        --version)  VERSION="$2"; shift 2 ;;
        --help|-h)
            sed -n '/^#/p' "$0" | head -12 | sed 's/^# //'
            exit 0
            ;;
        *) echo "Unknown argument: $1"; exit 1 ;;
    esac
done

exec "$PROJECT_ROOT/scripts/build-linux-local.sh" \
    --arch arm64 --version "$VERSION" --format deb
