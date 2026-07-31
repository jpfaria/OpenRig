#!/usr/bin/env bash
# release-downloads.sh — download counts for OpenRig GitHub releases
#
# Usage:
#   ./scripts/release-downloads.sh              latest 5 releases: tag + total downloads
#   ./scripts/release-downloads.sh -n 10         latest 10 releases
#   ./scripts/release-downloads.sh v0.1.1        one release: per-asset + total downloads
#
# Requires: gh (authenticated), jq.
set -euo pipefail

REPO="jpfaria/OpenRig"
LIMIT=5
VERSION=""

while [[ $# -gt 0 ]]; do
  case "$1" in
    -n) LIMIT="$2"; shift 2 ;;
    -*) echo "Unknown flag: $1" >&2; exit 1 ;;
    *) VERSION="$1"; shift ;;
  esac
done

if [[ -n "$VERSION" ]]; then
  tag="$VERSION"
  [[ "$tag" == v* ]] || tag="v$tag"
  gh api "repos/$REPO/releases/tags/$tag" | jq -r '
    "\(.tag_name)  (\(.published_at | split("T")[0]))",
    (.assets[] | "  \(.download_count)\t\(.name)"),
    "  ---",
    "  \(([.assets[].download_count] | add // 0))\ttotal"
  '
else
  gh api "repos/$REPO/releases?per_page=$LIMIT" | jq -r '
    .[] | "\(([.assets[].download_count] | add // 0))\t\(.tag_name)\t\(.published_at | split("T")[0])\(if .prerelease then "  (pre-release)" else "" end)"
  ' | column -t -s $'\t'
fi
