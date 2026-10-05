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

## Event context (#1070)

Every event carries two contexts, added by the `before_send` hook:

- `audio` — `backend` (cpal host or `jack`), `live_sample_rate` (`null` with no runtime), `devices[]` (`device_id`, `sample_rate`, `buffer_size_frames`, `bit_depth`) and `chains[]` (`id`, `description`, `enabled`, `io_bindings[]` with endpoints/channels, `blocks[]` as `<kind>/<model>`). Rebuilt on the GUI thread every time the runtime starts, syncs, upserts, removes a chain or stops — never on the audio thread.
- `host` — `cpu_brand`, `cpu_cores`, `cpu_physical_cores`, `memory_total_mb`, `process_memory_mb` (RSS) and `process_cpu_percent` (since the previous event), sampled when the event is built.

To read what reached Sentry from a terminal: `./scripts/sentry.py issues`, then `./scripts/sentry.py events OPENRIG-N` (skill `openrig-sentry`).
