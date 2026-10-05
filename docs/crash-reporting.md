# Crash reporting and logs (#1060)

## Session log on disk

Every run writes its log to a new file, and the newest 10 are kept:

| OS | Folder |
|---|---|
| macOS | `~/Library/Logs/OpenRig/` |
| Windows | `%APPDATA%\OpenRig\logs\` |
| Linux | `~/.local/share/openrig/logs/` |

Files are named `openrig-<unix seconds>.log`. Lines still go to stderr too.
A panic is written straight to the file (message, thread, backtrace) under a
`===== PANIC =====` marker, bypassing the async log queue so it survives the crash.

## Sentry

Release builds send panics and `log::error!` records to Sentry; lower levels
become breadcrumbs attached to the next event. The DSN comes from the
`SENTRY_DSN` repository secret, baked in at compile time by `release.yml`.
Builds without it (local `cargo run`) never send anything.

The audio thread never logs (invariant #8), so none of this touches it.

## Audio faults (#1065)

Audio overload (new xruns or underruns on a chain, detected by the GUI meter
poll) and an unhealthy audio backend are logged at `error!`, so each one is a
Sentry event. The overload message is constant per chain (`audio overload on
chain '<id>' (xrun/underrun)`) so Sentry groups occurrences; the counts ride in
the `warn!` breadcrumb right before it.

To read what reached Sentry from a terminal: `./scripts/sentry.py issues`, then `./scripts/sentry.py events OPENRIG-N` (skill `openrig-sentry`).
