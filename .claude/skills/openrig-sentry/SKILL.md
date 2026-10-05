---
name: openrig-sentry
description: Use when asked to look at OpenRig's Sentry — "olha o sentry", "analisa o sentry", "quais erros chegaram", a sound/crash bug reported from an installed build, or before diagnosing an audio fault that release builds report (xrun/underrun, backend loss, missing device, panic).
---

# OpenRig Sentry

## Overview

Release builds send panics and `log::error!` records to Sentry (`docs/crash-reporting.md`,
#1060/#1065); lower log levels ride along as breadcrumbs. `scripts/sentry.py` reads them
through the Sentry API — org `joao-paulo-faria`, project `openrig` — so nobody needs the web UI.

## Quick reference

| Goal | Command |
|---|---|
| Unresolved issues, last 14 days | `./scripts/sentry.py issues` |
| Other query / window | `./scripts/sentry.py issues -q "is:unresolved level:error" -p 24h` |
| Latest events of one issue: release, OS, device, breadcrumbs, stack | `./scripts/sentry.py events OPENRIG-2 -n 3 -b 30` |
| Mark resolved (needs `event:write`) | `./scripts/sentry.py resolve OPENRIG-2` |

Token: `SENTRY_AUTH_TOKEN`, or `token=` under `[auth]` in `~/.sentryclirc` (personal token,
scopes `org:read project:read event:read`). Never commit it. `SENTRY_ORG` / `SENTRY_PROJECT`
override the defaults.

## How to analyse

1. `issues` first; pick by count and `lastSeen`.
2. `events` on it: the breadcrumbs are the session log right before the error — what the
   chain was doing (restart, rebuild, preset switch) is usually there.
3. The same lines are in the session log on disk (`~/Library/Logs/OpenRig/openrig-*.log` on
   macOS) with everything below `error!`; use it for the full timeline.
4. Audio faults: follow the CLAUDE.md audio-incident rule — search `docs/audio-incidents/`
   for the symptom before any hypothesis, and record the Sentry issue id in the incident file.

## Common mistakes

- Reading only the event title: the counts of an overload are in the `warn!` breadcrumb
  right before it (`N new xrun(s), M new underrun(s)`), not in the grouped title.
- Treating a `missing input device` event as an audio bug: it fires when the interface is
  off or unplugged (#1069), not when the engine fails.
