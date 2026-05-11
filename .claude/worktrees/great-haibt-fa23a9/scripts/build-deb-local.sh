#!/usr/bin/env bash
# Build .deb packages for Linux using Docker.
#
# Usage:
#   ./scripts/build-deb-local.sh [--arch arm64|x86_64|all] [--version V] [--clean] [--nuke]
#
# Flags:
#   --clean  Forward to build-linux-local.sh: wipe target/ and output/ before
#            building. Use when you hit E0460/E0463 "possibly newer version
#            of crate" after merges (corrupted incremental cache).
#   --nuke   Forward to build-linux-local.sh: --clean + also wipe
#            ~/.cargo/registry/cache AND rebuild the Docker builder image.
#
# Output:
#   output/deb/openrig_{VERSION}_arm64.deb
#   output/deb/openrig_{VERSION}_amd64.deb

set -euo pipefail

PROJECT_ROOT="$(cd "$(dirname "$0")/.." && pwd)"
OUTPUT_DIR="$PROJECT_ROOT/output/deb"
VERSION="0.0.0-dev"
ARCH="all"
EXTRA_FLAGS=()

while [ $# -gt 0 ]; do
    case "$1" in
        --version)  VERSION="$2"; shift 2 ;;
        --arch)     ARCH="$2";    shift 2 ;;
        --clean)    EXTRA_FLAGS+=(--clean); shift ;;
        --nuke)     EXTRA_FLAGS+=(--nuke);  shift ;;
        --help|-h)
            sed -n '/^#/p' "$0" | head -14 | sed 's/^# //'
            exit 0
            ;;
        *) echo "Unknown argument: $1"; exit 1 ;;
    esac
done

mkdir -p "$OUTPUT_DIR"

if [ "$ARCH" = "all" ]; then
    ARCHS=(arm64 x86_64)
else
    ARCHS=("$ARCH")
fi

for A in "${ARCHS[@]}"; do
    "$PROJECT_ROOT/scripts/build-linux-local.sh" \
        --arch "$A" --version "$VERSION" --format deb \
        --output-dir "$OUTPUT_DIR" \
        "${EXTRA_FLAGS[@]+"${EXTRA_FLAGS[@]}"}"
done

echo ""
echo "Done. Packages in output/deb/:"
ls -lh "$OUTPUT_DIR"/openrig_*.deb 2>/dev/null | awk '{print "  " $NF, $5}'
echo ""
echo "To build the Orange Pi image:"
echo "  ./scripts/build-orange-pi-image.sh --local-deb $OUTPUT_DIR/openrig_${VERSION}_arm64.deb"
