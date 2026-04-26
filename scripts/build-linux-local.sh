#!/usr/bin/env bash
# Build Linux packages locally using Docker.
#
# Wraps scripts/package-linux.sh in the same Debian 12 container used by CI,
# so output is identical to GitHub Actions.
#
# Usage:
#   ./scripts/build-linux-local.sh [--arch arm64|x86_64] [--version V] [--format FORMAT] [--output-dir DIR] [--clean] [--nuke]
#
# Flags:
#   --clean  Remove target/ and output/ before building. Use when the build
#            fails with E0460 "possibly newer version of crate X" or E0463
#            "can't find crate" — those are signs of a corrupted incremental
#            cache after merges or cross-platform builds sharing the same
#            target/. Slower (full rebuild) but deterministic.
#   --nuke   --clean + also wipe ~/.cargo/registry/cache AND rebuild the
#            Docker builder image from scratch. Use only when --clean alone
#            doesn't resolve the cache corruption. Much slower (redownload
#            crates + rebuild image).
#
# Default output: output/linux/

set -euo pipefail

PROJECT_ROOT="$(cd "$(dirname "$0")/.." && pwd)"

# ── Defaults ──────────────────────────────────────────────────────────────────
ARCH="$(uname -m)"
VERSION="0.0.0-dev"
FORMAT="all"
OUTPUT_DIR="$PROJECT_ROOT/output/linux"
CLEAN=0
NUKE=0

# ── Parse args ────────────────────────────────────────────────────────────────
while [ $# -gt 0 ]; do
    case "$1" in
        --arch)       ARCH="$2"; shift 2 ;;
        --version)    VERSION="$2"; shift 2 ;;
        --format)     FORMAT="$2"; shift 2 ;;
        --output-dir) OUTPUT_DIR="$2"; shift 2 ;;
        --clean)      CLEAN=1; shift ;;
        --nuke)       CLEAN=1; NUKE=1; shift ;;
        --help|-h)
            sed -n '/^#/p' "$0" | head -20 | sed 's/^# //'
            exit 0
            ;;
        *) echo "Unknown argument: $1"; exit 1 ;;
    esac
done

# ── Optional cleanup before build ────────────────────────────────────────────
if [ "$CLEAN" = "1" ]; then
    echo "══════════════════════════════════════════"
    echo "  0/2  Cleaning caches"
    echo "══════════════════════════════════════════"
    echo "Removing $PROJECT_ROOT/target"
    rm -rf "$PROJECT_ROOT/target"
    echo "Removing $PROJECT_ROOT/output"
    rm -rf "$PROJECT_ROOT/output"
    if [ "$NUKE" = "1" ]; then
        echo "Removing $HOME/.cargo/registry/cache"
        rm -rf "$HOME/.cargo/registry/cache" 2>/dev/null || true
    fi
    echo ""
fi

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
DOCKER_BUILD_ARGS=()
if [ "$NUKE" = "1" ]; then
    echo "Removing existing builder image ($DOCKER_IMAGE)"
    docker rmi -f "$DOCKER_IMAGE" 2>/dev/null || true
    DOCKER_BUILD_ARGS+=(--no-cache)
fi
docker build --platform "$DOCKER_PLATFORM" -t "$DOCKER_IMAGE" \
    "${DOCKER_BUILD_ARGS[@]}" \
    -f "$DOCKERFILE" "$PROJECT_ROOT/docker"

# ── Run package-linux.sh inside Docker ───────────────────────────────────────
echo ""
echo "══════════════════════════════════════════"
echo "  2/2  Building packages inside Docker"
echo "══════════════════════════════════════════"
docker run --rm --platform "$DOCKER_PLATFORM" \
    -v "$PROJECT_ROOT:/workspace:delegated" \
    -v "$HOME/.cargo/registry:/root/.cargo/registry:delegated" \
    -v "$HOME/.cargo/git:/root/.cargo/git:delegated" \
    -e CARGO_NET_OFFLINE="${CARGO_NET_OFFLINE:-false}" \
    "$DOCKER_IMAGE" \
    bash -c "cd /workspace && ./scripts/package-linux.sh \
        --arch ${ARCH} --version ${VERSION} --format ${FORMAT} \
        --output-dir /workspace/${REL_OUTPUT}"
