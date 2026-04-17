#!/usr/bin/env bash
# Build .deb for arm64 and deploy to Orange Pi.
#
# Usage:
#   ./scripts/deploy-orange-pi.sh [--version V] [--host H]
#
# Defaults:
#   version: 0.0.0-dev
#   host:    root@192.168.15.145

set -euo pipefail

PROJECT_ROOT="$(cd "$(dirname "$0")/.." && pwd)"
VERSION="0.0.0-dev"
HOST="root@192.168.15.145"

while [ $# -gt 0 ]; do
    case "$1" in
        --version) VERSION="$2"; shift 2 ;;
        --host)    HOST="$2";    shift 2 ;;
        --help|-h)
            sed -n '1,8p' "$0" | grep '^#' | sed 's/^# \?//'
            exit 0 ;;
        *) echo "Unknown option: $1" >&2; exit 1 ;;
    esac
done

DEB="$PROJECT_ROOT/output/deb/openrig_${VERSION}_arm64.deb"

echo "==> Building .deb arm64 (version $VERSION)..."
"$PROJECT_ROOT/scripts/build-deb-local.sh" --version "$VERSION"

echo "==> Copying $DEB to $HOST:/tmp/..."
scp "$DEB" "$HOST:/tmp/"

echo "==> Installing and restarting service on $HOST..."
ssh "$HOST" "dpkg -i /tmp/openrig_${VERSION}_arm64.deb && systemctl restart openrig.service"

echo "==> Done. OpenRig $VERSION deployed to $HOST."
