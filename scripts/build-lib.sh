#!/usr/bin/env bash
set -euo pipefail

# Build native libraries for OpenRig plugins.
#
# Usage:
#   ./scripts/build-lib.sh <plugin|all>                          # build for current platform
#   ./scripts/build-lib.sh <plugin|all> --platform linux-x86_64  # Linux x86_64 via Docker
#   ./scripts/build-lib.sh <plugin|all> --platform linux-aarch64 # Linux ARM64 via Docker (QEMU)
#   ./scripts/build-lib.sh <plugin|all> --platform windows-x64   # Windows via Docker (mingw)
#   ./scripts/build-lib.sh <plugin|all> --platform all           # build for ALL platforms
#   ./scripts/build-lib.sh --list                                # list available plugins
#
# Examples:
#   ./scripts/build-lib.sh nam
#   ./scripts/build-lib.sh dragonfly-reverb zam-plugins
#   ./scripts/build-lib.sh all
#   ./scripts/build-lib.sh nam --platform linux-x86_64
#   ./scripts/build-lib.sh nam --platform all

PROJECT_ROOT="$(cd "$(dirname "$0")/.." && pwd)"
DOCKER_IMAGE="openrig-build-libs"

# Parse args
PLUGINS=()
PLATFORM=""

while [ $# -gt 0 ]; do
    case "$1" in
        --platform)
            PLATFORM="$2"
            shift 2
            ;;
        --list)
            # Forward to internal script
            bash "$PROJECT_ROOT/scripts/build-lib-internal.sh" --list
            exit 0
            ;;
        --help|-h)
            bash "$PROJECT_ROOT/scripts/build-lib-internal.sh" --help
            exit 0
            ;;
        *)
            PLUGINS+=("$1")
            shift
            ;;
    esac
done

if [ ${#PLUGINS[@]} -eq 0 ]; then
    bash "$PROJECT_ROOT/scripts/build-lib-internal.sh" --help
    exit 1
fi

# Detect platform if not specified
detect_platform() {
    local os=$(uname -s)
    local arch=$(uname -m)

    if [ "$os" = "Darwin" ]; then
        echo "macos-universal"
    elif [ "$os" = "Linux" ] && [ "$arch" = "x86_64" ]; then
        echo "linux-x86_64"
    elif [ "$os" = "Linux" ] && [ "$arch" = "aarch64" ]; then
        echo "linux-aarch64"
    else
        echo "unknown"
    fi
}

if [ -z "$PLATFORM" ]; then
    PLATFORM=$(detect_platform)
fi

echo "Platform: $PLATFORM"
echo "Plugins:  ${PLUGINS[*]}"
echo ""

# Determine output directory for a plugin + platform
# NAM goes to libs/nam/, everything else to libs/lv2/
output_dir_for_platform() {
    local plugin="$1"
    local platform="$2"
    if [ "$plugin" = "nam" ]; then
        echo "$PROJECT_ROOT/libs/nam/$platform"
    else
        echo "$PROJECT_ROOT/libs/lv2/$platform"
    fi
}

# --- macOS: build natively ---
build_native() {
    for plugin in "${PLUGINS[@]}"; do
        local out_dir
        out_dir=$(output_dir_for_platform "$plugin" "macos-universal")
        mkdir -p "$out_dir"

        echo "Building $plugin natively -> $out_dir"

        OUTPUT_DIR="$out_dir" \
        DEPS_DIR="$PROJECT_ROOT/deps" \
        CMAKE_EXTRA="-DCMAKE_OSX_ARCHITECTURES=arm64;x86_64 -DCMAKE_OSX_DEPLOYMENT_TARGET=11.0" \
            bash "$PROJECT_ROOT/scripts/build-lib-internal.sh" "$plugin"
    done
}

# --- Linux: build in Docker ---
build_docker() {
    local platform="$1"
    local docker_platform=""
    local docker_tag=""
    local -a extra_env=()

    case "$platform" in
        linux-x86_64)
            docker_platform="linux/amd64"
            docker_tag="$DOCKER_IMAGE:linux-amd64"
            ;;
        linux-aarch64)
            docker_platform="linux/arm64"
            docker_tag="$DOCKER_IMAGE:linux-arm64"
            ;;
        windows-x64)
            # Cross-compile from Linux using mingw-w64
            docker_platform="linux/amd64"
            docker_tag="$DOCKER_IMAGE:linux-amd64"
            extra_env=(
                -e "CROSS_COMPILE=x86_64-w64-mingw32"
                -e "CC=x86_64-w64-mingw32-gcc"
                -e "CXX=x86_64-w64-mingw32-g++"
                -e "CMAKE_EXTRA=-DCMAKE_SYSTEM_NAME=Windows -DCMAKE_C_COMPILER=x86_64-w64-mingw32-gcc -DCMAKE_CXX_COMPILER=x86_64-w64-mingw32-g++"
            )
            ;;
        *)
            echo "ERROR: Unsupported Docker platform: $platform"
            exit 1
            ;;
    esac

    # Build Docker image if needed
    echo "Building Docker image ($docker_platform)..."
    if ! docker build \
        --platform "$docker_platform" \
        -t "$docker_tag" \
        -f "$PROJECT_ROOT/docker/Dockerfile.build-libs" \
        "$PROJECT_ROOT"; then
        echo "ERROR: Docker build failed for $platform. Is Docker running?"
        return 1
    fi

    for plugin in "${PLUGINS[@]}"; do
        local out_dir
        out_dir=$(output_dir_for_platform "$plugin" "$platform")
        mkdir -p "$out_dir"

        echo "Building $plugin in Docker ($platform) -> $out_dir"

        docker run --rm \
            --platform "$docker_platform" \
            -v "$PROJECT_ROOT/deps:/build/deps:ro" \
            -v "$out_dir:/output" \
            ${extra_env[@]+"${extra_env[@]}"} \
            "$docker_tag" \
            "$plugin"
    done
}

ALL_PLATFORMS=(macos-universal linux-x86_64 linux-aarch64 windows-x64)

# --- Dispatch ---
if [ "$PLATFORM" = "all" ]; then
    echo "Building for ALL platforms: ${ALL_PLATFORMS[*]}"
    echo ""
    for plat in "${ALL_PLATFORMS[@]}"; do
        echo "============================================"
        echo "  Platform: $plat"
        echo "============================================"
        if [ "$plat" = "macos-universal" ]; then
            if [ "$(uname -s)" = "Darwin" ]; then
                build_native
            else
                echo "SKIP: macOS builds require running on macOS"
            fi
        else
            build_docker "$plat" || echo "FAILED: $plat (continuing...)"
        fi
        echo ""
    done
else
    case "$PLATFORM" in
        macos-universal)
            build_native
            ;;
        linux-x86_64|linux-aarch64|windows-x64)
            build_docker "$PLATFORM"
            ;;
        *)
            echo "ERROR: Unsupported platform: $PLATFORM"
            echo "Supported: macos-universal, linux-x86_64, linux-aarch64, windows-x64, all"
            exit 1
            ;;
    esac
fi

echo ""
echo "Build complete."
