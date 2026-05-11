# MK-300 v69 Effects Reference

This document stores the current MK-300 v69 reference the project is using for future native block work.

It is intentionally a product-reference document, not a schema contract.

Rules:
- Preserve module/type names as close as possible to the source unit.
- When the original documentation does not publish a numeric range, record that as `not specified in doc`.
- `type` below means MK-300 effect type inside a module, not OpenRig block type.

## Global / Home Screen

- `preset_bpm`: number, range `40..240`
- `tuner_volume`: number, range `0..100`
- `tuner_a_frequency_hz`: number, range `430..450`, default `440`
- `drum_volume`: number, range `0..100`

Notes:
- The documentation mentions `PAN` with an example where `-100` means only left output, but it does not publish the full numeric range in the extracted text.

## WAH Module

Types:
- `x_wah`
- `funk_wah`
- `slide_wah`
- `cry_wah`
- `wah_wah`
- `sense_wah`

### X-Wah
- `value`: pedal-controlled value, range `not specified in doc`
- `gain`: number, range `not specified in doc`
- `level`: number, range `not specified in doc`

### Funk-Wah
- `value`: pedal-controlled value, range `not specified in doc`
- `gain`: number, range `not specified in doc`
- `level`: number, range `not specified in doc`

### Slide-Wah
- `value`: pedal-controlled value, range `not specified in doc`
- `gain`: number, range `not specified in doc`
- `level`: number, range `not specified in doc`

### Cry-Wah
- `value`: pedal-controlled value, range `not specified in doc`
- `gain`: number, range `not specified in doc`
- `level`: number, range `not specified in doc`

### Wah-Wah (auto wah)
- `speed`: number or beat value when `sync=on`, range `not specified in doc`
- `q`: number, range `not specified in doc`
- `mix`: number, range `not specified in doc`
- `width`: number, range `not specified in doc`
- `level`: number, range `not specified in doc`
- `sync`: boolean, values `on|off`

### Sense-Wah
- `sense`: number, range `not specified in doc`
- `attack`: number, range `not specified in doc`
- `q`: number, range `not specified in doc`
- `f_peak`: number, range `not specified in doc`
- `mix`: number, range `not specified in doc`
- `width`: number, range `not specified in doc`
- `level`: number, range `not specified in doc`

## FX Module

### Lofi
- `bit`: number, range `not specified in doc`
- `level`: number, range `not specified in doc`
- `filter`: number, range `not specified in doc`

### Boost
- `gain`: number, range `not specified in doc`
- `level`: number, range `not specified in doc`

### A Boost
- `gain`: number, range `not specified in doc`
- `bass`: number, range `not specified in doc`
- `mid`: number, range `not specified in doc`
- `treble`: number, range `not specified in doc`
- `level`: number, range `not specified in doc`

### E Boost
- `gain`: number, range `not specified in doc`
- `bass`: number, range `not specified in doc`
- `mid`: number, range `not specified in doc`
- `treble`: number, range `not specified in doc`
- `level`: number, range `not specified in doc`

### B Boost
- `gain`: number, range `not specified in doc`
- `bass`: number, range `not specified in doc`
- `mid`: number, range `not specified in doc`
- `treble`: number, range `not specified in doc`
- `level`: number, range `not specified in doc`

### Boost ED
- `gain`: number, range `not specified in doc`
- `grit`: number, range `not specified in doc`
- `level`: number, range `not specified in doc`

### Compress
- `sustain`: number, range `not specified in doc`
- `attack`: number, range `not specified in doc`
- `wet_level`: number, range `not specified in doc`
- `blend`: number, range `not specified in doc`

### Compress Pro
- `ratio`: number, range `not specified in doc`
- `gain`: number, range `not specified in doc`
- `knee`: number, range `not specified in doc`
- `thd`: number, range `not specified in doc`
- `attack`: number, range `not specified in doc`
- `wet_level`: number, range `not specified in doc`
- `blend`: number, range `not specified in doc`

### F Compress
- `ratio`: number, range `not specified in doc`
- `gain`: number, range `not specified in doc`
- `knee`: number, range `not specified in doc`
- `thd`: number, range `not specified in doc`
- `attack`: number, range `not specified in doc`
- `tone`: number, range `not specified in doc`
- `wet_level`: number, range `not specified in doc`
- `blend`: number, range `not specified in doc`

### Pitch
- `high_pitch`: number, range `not specified in doc`
- `low_pitch`: number, range `not specified in doc`
- `high_level`: number, range `not specified in doc`
- `low_level`: number, range `not specified in doc`
- `dry_level`: number, range `not specified in doc`

### Octave
- `high_level`: number, range `not specified in doc`
- `low_level`: number, range `not specified in doc`
- `dry_level`: number, range `not specified in doc`

### Ring
- `freq`: number, range `not specified in doc`
- `mix`: number, range `not specified in doc`

## GATE Module

### AI Gate
- `gate`: number, range `not specified in doc`
- `bias`: number, range `not specified in doc`

### Soft Gate
- `thd`: number, range `not specified in doc`

### Hard Gate
- `thd`: number, range `not specified in doc`

### Pro Gate
- `att`: number, range `not specified in doc`
- `rel`: number, range `not specified in doc`
- `thd`: number, range `not specified in doc`
- `kw`: number, range `not specified in doc`
- `ratio`: number, range `not specified in doc`

## EQ Module

Types:
- `guitar_eq_6`
- `bass_eq_7`
- `normal_eq_10`

Notes:
- The documentation text references bands such as `31.25hz`, `250hz`, `1khz`, `2khz`, `4khz`, `8khz`, `16khz`.
- Per-band gain is numeric but explicit dB min/max is `not specified in doc`.

## MOD Module

Global note:
- `sync`: boolean, values `on|off`
- When `sync=on`, `speed` is displayed as beat divisions and follows the main BPM.

### Chorus
- `speed`: number or beat value when `sync=on`, range `not specified in doc`
- `depth`: number, range `not specified in doc`
- `mix`: number, range `not specified in doc`
- `sync`: boolean, values `on|off`

### Opto Tremolo
- `speed`: number or beat value when `sync=on`, range `not specified in doc`
- `depth`: number, range `not specified in doc`
- `level`: number, range `not specified in doc`
- `sync`: boolean, values `on|off`

Notes:
- Other MOD types from the source unit remain valid references, but the extracted effects-description text does not publish explicit numeric ranges for them.

## DLY Module

Global note:
- `sync`: boolean, values `on|off`
- When `sync=on`, `time` is shown as beat divisions such as quarter, dotted, triplet, and related note values.
- The extracted text does not publish explicit numeric ranges for `time`, `feedback`, or `mix`.

## REV Module

The documentation describes types such as:
- `room`
- `hall`
- `plate`
- `spring`
- `shimmer`
- `bloom`
- `cloud`
- `lofi`
- `swell`

Notes:
- The extracted text does not publish explicit numeric ranges for parameters such as `decay` or `mix`.

## VOL Module

Notes from the source:
- The volume module controls preset output volume.
- The `vol` parameter adjusts module output volume.
- The extracted text does not publish an explicit numeric range for module volume.
