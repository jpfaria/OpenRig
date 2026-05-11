# Backend Contract

This document is the quick reference for the current non-UI runtime model.

## File roles

### `project.yaml`

Runtime project file.

Contains:

- optional project name
- optional `device_settings`
- `chains`

Does not contain:

- external preset references for runtime
- user-authored chain IDs
- user-authored block IDs

### `config.yaml`

Project-local configuration file that lives next to `project.yaml`.

Current responsibility:

- `presets_path`

Example:

```yaml
presets_path: ./presets
```

### Preset files

Preset files live under `presets_path`.

They are used only for editing workflows:

- save a chain's blocks to a preset
- load a preset into a chain

They do not participate in runtime project resolution.

## Current project shape

```yaml
name: My Project

device_settings:
  - device_id: "coreaudio:..."
    sample_rate: 48000
    buffer_size_frames: 256

chains:
  - description: guitar 1
    enabled: true
    input_device_id: "coreaudio:..."
    input_channels: [0]
    output_device_id: "coreaudio:..."
    output_channels: [0, 1]
    output_mixdown: average
    blocks:
      - type: amp
        model: marshall_jcm_800_2203
        enabled: true
        params:
          volume: 70.0
          gain: 40.0
```

Notes:

- `output_mixdown` defaults to `average`.
- `device_settings` is optional.

## Validation rules

### Project

- a project must have at least one chain

### Chain

- `input_device_id` is required
- `output_device_id` is required
- `input_channels` is required
- `output_channels` is required
- channels in each list must be unique
- current support is mono or stereo only

### Active input conflicts

Two enabled chains may not use the same:

- `input_device_id`
- input channel

Disabled chains do not participate in this conflict check.

### Blocks

- blocks support `enabled`
- disabled blocks are skipped by validation/runtime layout flow
- block IDs are generated internally when YAML is loaded

## Device identity

Persist device selection with backend `device_id`.

Do not model runtime device selection around:

- `match_name`
- vendor display names
- hand-written aliases

Those can be shown in UI, but `device_id` is the canonical runtime identifier.

## Console behavior

`adapter-console`:

- accepts `--project <path>`
- accepts `--config <path>`
- defaults to local `project.yaml`
- defaults to local `config.yaml`

If no local file exists, it falls back to the repository-root example files.

## Global adapter persistence

Global adapter data is not part of the runtime project model.

Current examples:

- `~/.config/OpenRig/config.yaml`
- `~/.config/OpenRig/config.yaml`

These are adapter-side persistence files and should not shape the `project` crate model.
