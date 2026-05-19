# Configuration taxonomy — system vs project

OpenRig persists configuration in two places. This page is the short version of
[ADR 0003](adr/0003-system-vs-project-config.md); read the ADR for the reasoning, this
page for the working rule.

## The rule

> A setting belongs to **PROJECT** if the answer to *"if I send this `.openrig` to
> another machine, does this value have to travel with it?"* is **yes**. Otherwise it
> belongs to **SYSTEM**.

- **System** → `config.yaml` in the per-OS config dir. Belongs to the installation /
  machine / user.
- **Project** → fields inside `project.openrig` (see
  [project format](projects/project-openrig-format.md)). Belongs to the rig / setlist
  and travels with the file.
- **Precedence at load time** → project overrides system on dimensions both can
  describe.

## Where each thing lives

### System (`config.yaml`)

- `language` — UI locale.
- `recent_projects` — recently opened projects list.
- `paths` — asset roots (thumbnails, screenshots, metadata).
- `input_devices` / `output_devices` — per-machine audio device defaults.
- MIDI device profile (`midi-profile.yaml`) — which controller port to listen to.
- MIDI binding fallback (`midi-bindings.yaml`) — bindings used when the project has
  no `midi:` field.

### Project (`project.openrig`)

- `inputs` / `outputs` / `presets` — the rig.
- `device_settings` — see ADR 0001 (kept project-level for now; see ADR 0003 §
  Sub-question).
- `midi.bindings` — what each binding does *for this rig*.

## MIDI: which file, when

| File | Layer | Contents | Resolution |
|---|---|---|---|
| `project.openrig` → `midi.bindings` | Project | Bindings for this rig | First |
| `midi-bindings.yaml` (per-OS config dir) | System | Bindings fallback | Second |
| `examples/midi-map.default.yaml` (shipped) | Default | Standard shipped map | Third |
| `midi-profile.yaml` (per-OS config dir) | System | Which controller | Always |

The `input:` (controller name substring) is **never** overridden by the project — it's
your hardware. Bindings are owned by the project so the same setlist behaves identically
on every machine.

## Migration from a pre-#499 `midi-map.yaml`

On first load, an existing `midi-map.yaml` is split into:

- `midi-profile.yaml` — receives the `input:` field.
- `midi-bindings.yaml` — receives the `bindings:` field as the system fallback.

The original `midi-map.yaml` is deleted after a successful split. No user action.
