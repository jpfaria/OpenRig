# CLI and environment variables (adapter-gui)

The desktop app's binary is `adapter-gui` in a checkout and `openrig` inside the installed app (`OpenRig.app/Contents/MacOS/openrig`).

| Argument / variable | Effect |
|---|---|
| `openrig --project /path/project.yaml` | Opens the project directly, skipping the launcher |
| `openrig /path/project.yaml` (positional) | The same (legacy form, still accepted) |
| `OPENRIG_PROJECT_PATH=...` | The same (the env var ranks below the CLI) |
| `--config <PATH>` | The `config.yaml` to read instead of the app's own (see precedence below) |
| `OPENRIG_PLUGINS_ROOT=...` | The plugins folder, overriding Settings and the default `<data dir>/plugins` |
| `RUST_LOG=...` | Log filter (default `info`). Logging is non-blocking: records go through a bounded queue drained by a dedicated writer thread; if the stderr consumer is slower than the producers, records are dropped and a `[log-writer] N record(s) dropped` line reports the gap. Log calls never stall the GUI thread. |
| `OPENRIG_UPDATE_CURRENT_VERSION=0.0.1` | macOS only: the launcher's update check compares the latest GitHub release against this version instead of the compiled one, so the update button can be exercised without publishing a new release. Display is unchanged. |
| `--mcp` | **Override**: forces the MCP server up at `http://127.0.0.1:4123` for this run (the GUI keeps running) — see [`mcp.md`](mcp.md). Persistent enablement is `mcp_enabled` in `config.yaml`. |
| `--mcp=ADDR:PORT` | The MCP server on the given address (e.g. `--mcp=0.0.0.0:9000`), overriding config for this run. |
| `--midi` | **Override**: forces the MIDI/BLE-MIDI adapter up for this run, using the **resolved view** (ADR 0003): project bindings (from `project.yaml`'s `midi:` block) → system fallback (`midi-bindings.yaml`) → shipped default. The controller comes from `midi-profile.yaml`. Migrates a legacy `midi-map.yaml` on first launch. Persistent enablement is `midi_enabled` in `config.yaml`. See [`midi.md`](midi.md). |
| `--midi=PATH` | Direct legacy-file load (no migration, no resolution), overriding config for this run. Useful for testing an explicit map (e.g. `--midi=~/maps/chocolate.yaml`). A map that binds mixer strips also gets motor-fader / LED feedback on the outputs named by its `input:` (see [`midi.md`](midi.md)). |

> **These flags are overrides, not the only switch.** Packaged builds launch the binary with no arguments, so MIDI/MCP enablement is driven by the per-machine `config.yaml` master switches `midi_enabled` / `mcp_enabled` (both default `false`; toggle them in Settings or by hand). A present `--midi` / `--mcp` flag forces the subsystem on for that single run regardless of config. See [`config-taxonomy.md`](config-taxonomy.md).

For headless offline rendering, see the **`openrig-render`** binary documented in [`render.md`](render.md). It is a separate executable shipped by `crates/adapter-render` — no GUI, no audio device, no MCP, no MIDI.

## Precedence

- **Project path:** `--project <PATH>` > positional > `OPENRIG_PROJECT_PATH` (the last CLI form wins). The resolved path is **validated** (`validate_project_path`): missing → `project file not found: <path>`; not a file → `project path is not a file: <path>`. An invalid path **does not crash the app** — it logs the error and falls back to the launcher.
- **`config.yaml`:** `--config <PATH>` > a `config.yaml` in the working directory (the development override) > the app's own config (`~/.openrig/config.yaml` on macOS and Linux, `%APPDATA%\OpenRig\config.yaml` on Windows) — the file **Settings → Paths** writes, and where the plugin catalog's `paths.plugins_path` comes from.
- **`--config` does not isolate a run.** Every setting the app saves (Appearance, Language, Paths, audio devices) and most reads still go to the app's own `config.yaml`, whatever `--config` says. A run that must not touch the owner's config (an agent checking a screen) sets `HOME` to a scratch directory holding a copy of `config.yaml` under `.openrig/`, with `midi_enabled: false` so it does not answer the owner's controller, and opens a copied project with `--project`.

Parsing lives in `crates/adapter-gui/src/cli.rs` and `cli_project_open.rs`.

## No autosave

The project file is written **only** on an explicit save: the Save button, the MCP `save_project` tool or a MIDI binding to `SaveProject` (all three dispatch `ProjectCommand::SaveProject`). Edits only flip the unsaved-changes flag (`sync_project_dirty()`). A leftover `--auto-save` on a command line is ignored like any unknown flag.
