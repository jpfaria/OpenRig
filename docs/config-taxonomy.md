# Configuration taxonomy — system vs project

OpenRig persists configuration in two places. This page is the short version of
[ADR 0003](adr/0003-system-vs-project-config.md); read the ADR for the reasoning, this
page for the working rule.

## The rule

> A setting belongs to **PROJECT** if the answer to *"if I send this `project.yaml` to
> another machine, does this value have to travel with it?"* is **yes**. Otherwise it
> belongs to **SYSTEM**.

- **System** → `config.yaml` in the per-OS config dir. Belongs to the installation /
  machine / user.
- **Project** → fields inside `project.yaml` (see
  [project format](projects/project-format.md)). Belongs to the rig / setlist
  and travels with the file.
- **Precedence at load time** → project overrides system on dimensions both can
  describe.

## Where each thing lives

### System (`config.yaml`)

- `language` — UI locale.
- `recent_projects` — recently opened projects list. Opening, saving or
  removing a project writes ONLY this list (read-modify-write). The
  GUI's in-memory config is loaded at boot and is stale for every other
  section, so writing it whole would overwrite edits made elsewhere (an I/O
  binding changed over MCP, for one).
- `paths` — asset roots (thumbnails, screenshots, metadata) plus four
  user-overridable directories: `presets_path` (project presets),
  `plugins_path` (NAM/IR/LV2 packs),
  `evaluations_path` (tone-analyzer outputs),
  `backing_tracks_path` (the user's own backing tracks). Each defaults to
  a folder under the OS data root (`~/Library/Application
  Support/OpenRig`, `%APPDATA%\OpenRig`, `~/.local/share/openrig`)
  and is machine-local per ADR 0003 — never travels with
  `project.yaml`.
- `input_devices` / `output_devices` — per-device audio settings: `device_id`,
  `name`, `sample_rate`, `buffer_size_frames`, `bit_depth`.
- `io_bindings` — the I/O binding registry ([ADR 0004](adr/0004-io-binding-registry.md)):
  each binding has an `id`, a `name` and the input / output endpoints of this
  machine's interfaces. A project refers to a binding only by `id` and
  endpoint name, so the physical devices never travel with `project.yaml`.
- `midi_devices` — the MIDI ports this machine has seen: `port_key`, an
  editable `alias`, and `enabled` (which ports the adapter listens to).
- `midi_enabled` / `mcp_enabled` — master switches for the
  MIDI/BLE-MIDI adapter and the MCP server. Both default `false`. Whether
  a given machine drives OpenRig over MIDI or exposes the MCP server is a
  per-machine call (a stage Mac wants MIDI; a CI box does not), so it lives
  here, not in `project.yaml`. The `--midi` / `--mcp` CLI flags override
  these for a single run (dev convenience). Distinct from the per-port
  `midi_devices[].enabled` selection, which only picks *which* ports the
  enabled adapter listens to.
- `metronome` — practice tempo and click preferences: `bpm`,
  `beats_per_bar`, `subdivision`, `timbre`, `volume`, `count_in` and
  `output_device`. A tempo you practise at belongs to you, not to the rig, and
  the output device it clicks through only exists on this machine. The on/off
  flag is deliberately **not** persisted: the metronome always opens off, so no
  config file can make a session start clicking.
- `player` — the backing-track player's `volume` and `output_device`. Speed,
  pitch and loop are not persisted: they belong to the track and reset when
  another one loads, and the player always opens stopped.
- `mixer` — the global mixer: a list of `{id, gain_db, muted, soloed}`,
  one per strip the user moved. `id` addresses a configured endpoint
  (`in:<channels>@<device>` / `out:<channels>@<device>`), so it only exists
  on this machine. Strips at unity, unmuted and not soloed are not stored; a missing
  entry means 0 dB, so a config without `mixer` changes nothing.
- `tone3000` — `api_key`, the user's own TONE3000 Secret Key (`t3k_cs_…`)
  used by the in-app browser ([tone3000.md](tone3000.md)). Each user brings
  their own key, so it never travels with `project.yaml`; the UI only shows
  whether one is set.
- `crash_reporting` — where this machine sends crash reports: `provider`
  (`sentry` by default, or `none`) and an optional `dsn` that overrides the
  one baked into release builds. See [crash-reporting.md](crash-reporting.md).
- MIDI device profile (`midi-profile.yaml`) — which controller port to listen to.
- MIDI binding fallback (`midi-bindings.yaml`) — bindings used when the project has
  no `midi:` field.

### Project (`project.yaml`)

- `inputs` / `outputs` / `presets` / `chain-order` — the rig. An input names
  its I/O binding (`io`) and endpoint from the system registry.
- `midi.bindings` — what each binding does *for this rig*.

## MIDI: which file, when

| File | Layer | Contents | Resolution |
|---|---|---|---|
| `project.yaml` → `midi.bindings` | Project | Bindings for this rig | First |
| `midi-bindings.yaml` (per-OS config dir) | System | Bindings fallback | Second |
| `examples/midi-map.default.yaml` (shipped) | Default | Standard shipped map | Third |
| `midi-profile.yaml` (per-OS config dir) | System | Which controller | Always |

The `input:` (controller name substring) is **never** overridden by the project — it's
your hardware. Bindings are owned by the project so the same setlist behaves identically
on every machine.

## Migration from a legacy `midi-map.yaml`

On first load, an existing `midi-map.yaml` is split into:

- `midi-profile.yaml` — receives the `input:` field.
- `midi-bindings.yaml` — receives the `bindings:` field as the system fallback.

The original `midi-map.yaml` is deleted after a successful split. No user action.
