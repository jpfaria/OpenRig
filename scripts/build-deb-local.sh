#!/usr/bin/env bash
# Build .deb packages for all Linux architectures (arm64 + x86_64) using Docker.
#
# Usage:
#   ./scripts/build-deb-local.sh [--version V]
#
# Output:
#   dist/openrig_{VERSION}_arm64.deb
#   dist/openrig_{VERSION}_amd64.deb

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

for ARCH in arm64 x86_64; do
    "$PROJECT_ROOT/scripts/build-linux-local.sh" \
        --arch "$ARCH" --version "$VERSION" --format deb
done

echo ""
echo "To build the Orange Pi image:"
echo "  ./scripts/build-orange-pi-image.sh --local-deb dist/openrig_${VERSION}_arm64.deb"
