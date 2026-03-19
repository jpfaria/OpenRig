# ADR 0002: Device Identity, Routing, and Conflict Rules

## Status

Accepted

## Context

Audio device names are not reliable identifiers. Two identical interfaces can appear with the same name, and `match_name` is not enough to distinguish them.

The project also needs a clear conflict rule for multi-track use. The important constraint is physical input ownership, not preset reuse.

## Decision

### Device identity

Persist audio devices by backend `device_id`.

Do not use `match_name` as the primary identifier in project data.

### Routing

A track defines its own routing directly:

- `input_device_id`
- `input_channels`
- `output_device_id`
- `output_channels`

There is no separate logical `inputs` layer in the runtime project model.

### Optional device overrides

`device_settings` is optional project-level configuration keyed by `device_id`.

It is used to override device stream configuration such as:

- `sample_rate`
- `buffer_size_frames`

If `device_settings` is omitted for a device, the backend defaults are used.

### Conflict rule

Two enabled tracks may not claim the same:

- `input_device_id`
- input channel

Disabled tracks are ignored by this conflict rule.

### Layout support

Current validation and runtime support:

- mono: 1 channel
- stereo: 2 channels

Anything else is invalid for now.

## Consequences

- Two identical interfaces can coexist when their `device_id` values differ.
- The same preset content can be used by multiple tracks as long as they do not claim the same active physical input channel.
- The validation rule is objective and tied to hardware ownership, not musical intent.
- Device override configuration stays optional and does not bloat minimal projects.
