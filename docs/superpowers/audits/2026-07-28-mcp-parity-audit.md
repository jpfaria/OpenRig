# MCP parity audit (#829)

**Goal:** everything the app can do must be reachable over MCP — both the
actions (`Command`) and the observations (`QueryKind`).

Supersedes the variant inventory in `2026-05-14-command-audit.md`, which
counted 25 commands (the enum has 83 now). That document stays as the
record of how the callback → command mapping was first derived.

## How parity is enforced (not audited by hand)

| Bus | Guard | Where |
|---|---|---|
| Write | MCP builds one tool per `Command` variant from `command_schema`; a test pins `tools().len() == COMMAND_VARIANT_COUNT` and that every variant survives schema derivation | `adapter-mcp/src/tools_tests.rs` |
| Read | `resources::uri_for(&QueryKind)` matches **without a wildcard arm** — a new query kind that is not exposed fails the build; a round-trip test resolves every kind back through `kind_for_uri` | `adapter-mcp/src/resources.rs`, `resources_tests.rs` |

So the recurring question is never "is the adapter complete?" — it is
"did the capability enter the bus at all?".

## Sweep method

Every `crates/adapter-gui/src/*.rs` was checked for a reference to
`Command::` / `QueryKind` / `dispatch`. Modules with none are either
presentation (view models, layout math, label formatting, i18n, asset
lookup) or a capability that never entered the bus. Only the second
class is a gap.

## Gaps found and closed in #829

| Capability | Was | Now |
|---|---|---|
| Tuner readings (note / cents / frequency per tap) | `tuner_session.rs`, GUI-only; only `SetTunerEnabled` existed | `QueryKind::TunerReadings` → `openrig://tuner` |
| Spectrum readings (per-band levels / peaks) | `spectrum_session.rs`, GUI-only | `QueryKind::SpectrumReadings` → `openrig://spectrum` |
| DI loop state (playing, playback peaks, source) | `di_meter.rs` + meter poll, GUI-only | `QueryKind::DiLoopState` → `openrig://di` |
| Chain latency probe | `latency_probe.rs` ran `engine::probe` in the click handler, with its own rate/buffer resolution | `QueryKind::ChainLatency` → `openrig://chains/{chain}/latency`; resolution moved to `application::query_latency` (one implementation) |
| Audio device re-enumeration (USB hot-swap) | two Slint callbacks did the work inline | `SettingsCommand::RefreshAudioDevices`; the work moved to `device_refresh_apply::refresh_now`, which both the callbacks and the `AudioDevicesRefreshed` event path call |

## Explicitly not a gap

- **Native VST3 editor.** #780 removed it — a VST3 block is edited through
  OpenRig's own knob editor like any other block. The May audit's
  `on_open_vst3_editor` row no longer describes the code.
- **Window open/close, navigation, search fields, drafts.** Screen
  concerns by the GUI law; the state they eventually commit is already a
  command.
- **Plugin screenshots / homepage links** (`plugin_info.rs`). Presentation
  assets resolved from the catalog id a client can already read via
  `openrig://plugins/{id}`.

## Re-running the sweep

```sh
cd crates/adapter-gui/src
for f in *.rs; do case "$f" in *tests*) continue;; esac; \
  grep -q "Command::\|QueryKind\|dispatch" "$f" || echo "$f"; done
```

Each name printed is either presentation or a gap — decide per file, never
by a regex heuristic (the rule that forbids scripted audits of the GUI law
applies here too).
