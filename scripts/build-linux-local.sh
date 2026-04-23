#!/usr/bin/env bash
# Build Linux packages locally using Docker.
#
# Wraps scripts/package-linux.sh in the same Debian 12 container used by CI,
# so output is identical to GitHub Actions.
#
# Usage:
#   ./scripts/build-linux-local.sh [--arch arm64|x86_64] [--version V] [--format FORMAT] [--output-dir DIR]
#
# Default output: output/linux/

set -euo pipefail

PROJECT_ROOT="$(cd "$(dirname "$0")/.." && pwd)"

# ── Defaults ──────────────────────────────────────────────────────────────────
ARCH="$(uname -m)"
VERSION="0.0.0-dev"
FORMAT="all"
OUTPUT_DIR="$PROJECT_ROOT/output/linux"

# ── Parse args ────────────────────────────────────────────────────────────────
while [ $# -gt 0 ]; do
    case "$1" in
        --arch)       ARCH="$2"; shift 2 ;;
        --version)    VERSION="$2"; shift 2 ;;
        --format)     FORMAT="$2"; shift 2 ;;
        --output-dir) OUTPUT_DIR="$2"; shift 2 ;;
        --help|-h)
            sed -n '/^#/p' "$0" | head -12 | sed 's/^# //'
            exit 0
            ;;
        *) echo "Unknown argument: $1"; exit 1 ;;
    esac
done

# Normalise arch → Docker platform
case "$ARCH" in
    arm64|aarch64)       ARCH="aarch64"; DOCKER_PLATFORM="linux/arm64"  ;;
    x86_64|amd64|x64)   ARCH="x86_64";  DOCKER_PLATFORM="linux/amd64"  ;;
    *) echo "Unsupported arch: $ARCH"; exit 1 ;;
esac

# OUTPUT_DIR must be inside PROJECT_ROOT (Docker mount boundary)
# Convert to path relative to PROJECT_ROOT for use inside the container
REL_OUTPUT="${OUTPUT_DIR#$PROJECT_ROOT/}"
if [[ "$REL_OUTPUT" == /* ]]; then
    echo "ERROR: --output-dir must be inside the project root ($PROJECT_ROOT)"
    exit 1
fi

DOCKER_IMAGE="openrig-linux-builder:${ARCH}"
DOCKERFILE="$PROJECT_ROOT/docker/Dockerfile.linux-builder"

echo "OpenRig — Linux package builder (Docker)"
echo "Arch:     $ARCH"
echo "Version:  $VERSION"
echo "Format:   $FORMAT"
echo "Output:   $OUTPUT_DIR"
echo ""

# ── Check prerequisites ───────────────────────────────────────────────────────
if ! command -v docker >/dev/null 2>&1; then
    echo "ERROR: Docker is required."
    exit 1
fi

# ── Build Docker image (cached after first run) ───────────────────────────────
echo "══════════════════════════════════════════"
echo "  1/2  Preparing Docker build environment"
echo "══════════════════════════════════════════"
docker build --platform "$DOCKER_PLATFORM" -t "$DOCKER_IMAGE" \
    -f "$DOCKERFILE" "$PROJECT_ROOT/docker"

# ── Run package-linux.sh inside Docker ───────────────────────────────────────
echo ""
echo "══════════════════════════════════════════"
echo "  2/2  Building packages inside Docker"
echo "══════════════════════════════════════════"
docker run --rm --platform "$DOCKER_PLATFORM" \
    -v "$PROJECT_ROOT:/workspace:delegated" \
    -e CARGO_NET_OFFLINE="${CARGO_NET_OFFLINE:-false}" \
    "$DOCKER_IMAGE" \
    bash -c "cd /workspace && ./scripts/package-linux.sh \
        --arch ${ARCH} --version ${VERSION} --format ${FORMAT} \
        --output-dir /workspace/${REL_OUTPUT}"
