---
name: openrig-release-downloads
description: Use when asked for OpenRig's GitHub release download counts — "how many downloads does the latest release have", "download stats for the last N releases", "how many people downloaded v0.1.1" — instead of hand-rolling gh api calls each time.
---

# OpenRig Release Downloads

## Overview

`scripts/release-downloads.sh` wraps `gh api` to report download counts for
OpenRig's GitHub releases — the single source of truth for this data, used by
the marketing site (`site/app.js`, live-fetched via the GitHub Releases API)
and safe to reuse from a chat answer or a script.

## When to use

- "download stats for the last releases"
- "how many downloads does v0.1.1 have"
- Any request for release download numbers — don't call `gh api repos/.../releases` ad hoc; use the script so multi-asset sums and pre-release filtering stay consistent.

## Quick reference

| Goal | Command |
|---|---|
| Latest 5 releases (tag, date, total downloads) | `./scripts/release-downloads.sh` |
| Latest N releases | `./scripts/release-downloads.sh -n 10` |
| One release, per-asset breakdown | `./scripts/release-downloads.sh v0.1.1` (accepts `0.1.1` too) |

Requires `gh` (authenticated) and `jq`. Downloads are summed across ALL assets
of a release — a release with both a macOS and a Windows build reports the
combined total, not just one platform.

## Common mistakes

- Reading `download_count` off a single asset and reporting it as "the
  release's downloads" — sum every asset (`gh api .../releases/tags/<tag>`
  → `[.assets[].download_count] | add`).
- Forgetting pre-releases show up in `gh api repos/.../releases` too — the
  script flags them `(pre-release)` in the list view; don't silently fold
  them into "stable" totals without noting that.
