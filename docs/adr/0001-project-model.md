# ADR 0001: Project Runtime Model

## Status

Accepted

## Context

OpenRig originally evolved around a `setup`-style model and, at different times, tracks referenced presets directly. That made the runtime model less clear than it needed to be.

The current system needs a single runtime source of truth that:

- is easy to validate
- is easy to load into the engine
- does not depend on external preset indirection
- still allows preset import/export workflows

## Decision

The runtime model is:

- `Project`
- `Track`
- `AudioBlock`

Rules:

1. `project.yaml` is the runtime source of truth for a project.
2. A `Project` contains:
   - optional `name`
   - optional `device_settings`
   - required `tracks`
3. A `Track` contains:
   - optional `description`
   - `enabled`
   - `input_device_id`
   - `input_channels`
   - `output_device_id`
   - `output_channels`
   - `blocks` (YAML alias: `stages`)
   - `output_mixdown`
4. Tracks own their blocks directly.
5. Presets are not part of the runtime dependency graph.
6. Preset files are only used to:
   - save a track's blocks
   - load blocks into a track
   - replace a track's blocks
7. User-authored YAML does not provide runtime `TrackId` or `BlockId`. Those are generated when loading the project.

## Example

`project.yaml`

```yaml
device_settings:
  - device_id: "coreaudio:..."
    sample_rate: 48000
    buffer_size_frames: 256

tracks:
  - description: guitar 1
    enabled: true
    input_device_id: "coreaudio:..."
    input_channels: [0]
    output_device_id: "coreaudio:..."
    output_channels: [0, 1]
    stages:
      - type: amp
        model: marshall_jcm_800_2203
        enabled: true
        params:
          volume: 70.0
          gain: 40.0
```

`config.yaml`

```yaml
presets_path: ./presets
```

## Consequences

- The engine can build runtime state directly from the project.
- The project file stays focused on routing and active processing state.
- Multiple tracks can use the same preset content only by copying/loading blocks into each track.
- Saving a preset does not change the runtime model.
- Loading a preset is an editing operation on a track, not a runtime reference.
