#!/usr/bin/env bash
# Build an arm64 .deb package locally using Docker.
#
# Reproduces the exact same environment and steps as the GitHub Actions
# release workflow (build-linux-aarch64 job) so the resulting .deb is
# identical to what CI would produce.
#
# Requires Docker on macOS Apple Silicon (arm64 containers run natively).
#
# Usage:
#   ./scripts/build-deb-local.sh                  # version 0.0.0-dev
#   ./scripts/build-deb-local.sh --version 1.2.3  # specific version
#
# Output:
#   output/deb/openrig_<VERSION>_arm64.deb

set -euo pipefail

PROJECT_ROOT="$(cd "$(dirname "$0")/.." && pwd)"
OUTPUT_DIR="$PROJECT_ROOT/output/deb"
DOCKER_IMAGE="openrig-builder:arm64"
VERSION="0.0.0-dev"

# ── Parse args ────────────────────────────────────────────────────────────────
while [ $# -gt 0 ]; do
    case "$1" in
        --version)
            VERSION="$2"
            shift 2
            ;;
        --help|-h)
            grep '^#' "$0" | head -14 | sed 's/^# //'
            exit 0
            ;;
        *)
            echo "Unknown argument: $1"
            exit 1
            ;;
    esac
done

echo "OpenRig — Local .deb Builder (arm64)"
echo "Version: $VERSION"
echo ""

# ── Check prerequisites ──────────────────────────────────────────────────────
if ! command -v docker >/dev/null 2>&1; then
    echo "ERROR: Docker is required. Install with: brew install docker"
    exit 1
fi

# ── Build Docker image (cached after first run) ──────────────────────────────
echo "══════════════════════════════════════════"
echo "  1/4  Preparing Docker build environment"
echo "══════════════════════════════════════════"

DOCKERFILE="$PROJECT_ROOT/scripts/Dockerfile.deb-builder"
cat > "$DOCKERFILE" <<'DKEOF'
FROM debian:12

ENV DEBIAN_FRONTEND=noninteractive
ENV CARGO_TERM_COLOR=always

# Build deps for Debian 12 (Bookworm) — glibc 2.36 compatible
RUN apt-get update && apt-get install -y \
    curl \
    libasound2-dev libudev-dev pkg-config \
    libfontconfig1-dev libseat-dev \
    libxkbcommon-dev libinput-dev libgbm-dev \
    libjack-jackd2-dev \
    fakeroot rpm ruby-dev build-essential \
    librsvg2-bin imagemagick \
    && rm -rf /var/lib/apt/lists/*

RUN gem install fpm --no-document

# Install Rust (stable)
RUN curl --proto '=https' --tlsv1.2 -sSf https://sh.rustup.rs | sh -s -- -y
ENV PATH="/root/.cargo/bin:${PATH}"

WORKDIR /workspace
DKEOF

docker build --platform linux/arm64 -t "$DOCKER_IMAGE" -f "$DOCKERFILE" "$PROJECT_ROOT/scripts"

# ── Build inside Docker ──────────────────────────────────────────────────────
echo ""
echo "══════════════════════════════════════════"
echo "  2/4  Building OpenRig (cargo build --release)"
echo "══════════════════════════════════════════"

mkdir -p "$OUTPUT_DIR"

docker run --rm --platform linux/arm64 \
    -v "$PROJECT_ROOT:/workspace:delegated" \
    -e VERSION="$VERSION" \
    "$DOCKER_IMAGE" \
    bash -c '
set -euo pipefail

echo ">>> Rust: $(rustc --version)"
echo ">>> Arch: $(uname -m)"
echo ">>> Building..."

cargo build --release -p adapter-gui

echo ""
echo "══════════════════════════════════════════"
echo "  3/4  Staging install tree"
echo "══════════════════════════════════════════"

S=dist/stage
rm -rf dist
mkdir -p "$S/usr/bin"
mkdir -p "$S/usr/lib/openrig/libs/lv2"
mkdir -p "$S/usr/lib/openrig/libs/nam"
mkdir -p "$S/usr/share/openrig/data"

cp target/release/adapter-gui    "$S/usr/bin/openrig"
cp -r libs/lv2/linux-aarch64     "$S/usr/lib/openrig/libs/lv2/linux-aarch64"
cp -r libs/nam/linux-aarch64     "$S/usr/lib/openrig/libs/nam/linux-aarch64"
cp -r data/lv2                   "$S/usr/share/openrig/data/lv2"
cp -r assets                     "$S/usr/share/openrig/assets"
cp -r captures                   "$S/usr/share/openrig/captures"

echo ""
echo "══════════════════════════════════════════"
echo "  4/4  Creating .deb with fpm"
echo "══════════════════════════════════════════"

fpm -s dir -t deb \
    -n openrig -v "${VERSION}" \
    --architecture arm64 \
    --description "OpenRig virtual guitar pedalboard" \
    --url "https://github.com/jpfaria/OpenRig" \
    --maintainer "Joao Paulo Faria" \
    --category sound \
    --depends libasound2 \
    --deb-no-default-config-files \
    -C dist/stage \
    --package "output/deb/openrig_${VERSION}_arm64.deb" \
    usr

echo ""
echo ">>> .deb created: output/deb/openrig_${VERSION}_arm64.deb"
ls -lh "output/deb/openrig_${VERSION}_arm64.deb"
'

echo ""
echo "Done: $OUTPUT_DIR/openrig_${VERSION}_arm64.deb"
echo ""
echo "Use with build-orange-pi-image.sh:"
echo "  ./scripts/build-orange-pi-image.sh --local-deb $OUTPUT_DIR/openrig_${VERSION}_arm64.deb"
