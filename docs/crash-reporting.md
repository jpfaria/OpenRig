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

## Crash reporter

The app reports through a vendor-neutral `CrashReporter` trait
(`crates/adapter-gui/src/crash_reporter.rs`): `capture` (one report with its
contexts), `breadcrumb` and `flush`. Sentry is one implementation
(`crash_reporter_sentry.rs`, the only production file that names the vendor);
another vendor is a sibling implementation plus one arm in
`crash_reporting::choose`.

Panics and `log::error!` records become reports; `warn!`/`info!` become
breadcrumbs; `debug!`/`trace!` stay local. A failing reporter loses the
report, never the app, and the session file on disk always gets everything
first.

Which reporter runs is a per-machine choice in `config.yaml`:

```yaml
crash_reporting:
  provider: sentry   # or: none
  dsn: https://<key>@<host>/<project>   # optional
```

With no `dsn`, release builds use the DSN baked in at compile time from the
`SENTRY_DSN` repository secret (`release.yml`); local builds have none, so
they send nothing unless `config.yaml` names one. `provider: none` turns
reporting off. A DSN that does not parse turns reporting off with a warning.

The audio thread never logs, so none of this touches it.

## Audio faults (#1065)

Audio overload (new xruns or underruns on a chain, detected by the GUI meter
poll) and an unhealthy audio backend are logged at `error!`, so each one is a
Sentry event. The overload message is constant per chain (`audio overload on
chain '<id>' (xrun/underrun)`) so Sentry groups occurrences; the counts ride in
the `warn!` breadcrumb right before it.

## Report context

Every report carries two contexts (`crash_context::contexts`):

- `audio` — `backend` (cpal host or `jack`), `live_sample_rate` (`null` with no runtime), `devices[]` (`device_id`, `sample_rate`, `buffer_size_frames`, `bit_depth`) and `chains[]` (`id`, `description`, `enabled`, `io_bindings[]` with endpoints/channels, `blocks[]` as `<kind>/<model>`). Rebuilt on the GUI thread every time the runtime starts, syncs, upserts, removes a chain or stops — never on the audio thread.
- `host` — `cpu_brand`, `cpu_cores`, `cpu_physical_cores`, `memory_total_mb`, `process_memory_mb` (RSS) and `process_cpu_percent` (since the previous report), sampled when the report is built; the process stats say `process_sampling: busy` when another thread is sampling.

Covered headless in CI by `crates/adapter-gui/tests/issue_1070_crash_reporting.rs` (config choice, the log bridge over a recording reporter, the Sentry implementation over sentry's in-memory transport) and `crash_context_publish_tests.rs` (a runtime teardown publishes the session). To check a real event end to end, set `OPENRIG_SENTRY_SMOKE_DSN=<dsn>` and run `cargo test -p adapter-gui --test issue_1070_crash_reporting smoke`: it sends one event titled `issue-1070 smoke: …` (without the variable the test prints a SKIP line). Read it with `./scripts/sentry.py issues`, then delete that issue.

To read what reached Sentry from a terminal: `./scripts/sentry.py issues`, then `./scripts/sentry.py events OPENRIG-N` (skill `openrig-sentry`).
