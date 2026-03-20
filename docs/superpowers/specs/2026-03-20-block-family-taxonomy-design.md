# Block Family Taxonomy Reorganization

## Context

OpenRig today mixes three different concepts in the same contract:

- technical infrastructure crates
- product-facing block families
- concrete models

This leak is visible in the current public contract:

- `compressor`, `gate`, `tremolo`, and `tuner` still appear as `effect_type`
- GUI compensates for that by remapping these values into product labels
- `IR` does not exist yet as a first-class block family
- `WAH` does not exist yet as a first-class block family
- `block-core` is a technical dependency, but the system shape still encourages it to leak into the product taxonomy

The result is a contract that is harder to evolve, harder to expose cleanly in the GUI, and not aligned with market-facing pedalboard categories.

## Goals

- Separate technical infrastructure from public block families
- Keep `NAM`, `IR`, and `FULL_RIG` as explicit product concepts
- Add `block-ir` and `block-wah`
- Rename `audio-ir` to `ir`
- Make the core expose stable family-level `effect_type` values
- Make the GUI reflect those families instead of hardcoding corrective remaps
- Keep `volume` inside `utility`
- Keep `eq` inside `filter`

## Non-Goals

- No new generic `FX` family
- No taxonomy reset for every crate in the workspace
- No redesign of the `select` block in this spec
- No routing redesign in this spec
- No model-level renames beyond what is required by family cleanup

## Approved Direction

### Technical Infrastructure

These are internal support crates and must not be treated as product families:

- `block-core`
- `asset-runtime`
- `ir`
- `nam`

### Public Block Families

These are the product-facing families the platform should expose:

- `amp_head`
- `amp_combo`
- `cab`
- `ir`
- `full_rig`
- `nam`
- `drive`
- `dynamics`
- `filter`
- `wah`
- `modulation`
- `delay`
- `reverb`
- `pitch`
- `utility`
- `routing`

### Family Rules

- `IR` is a first-class family, separate from `cab`
- `NAM` is a first-class family
- `FULL_RIG` is a first-class family
- `volume` stays inside `utility`
- `eq` stays inside `filter`
- `block-core` never appears as a user-facing category

## Current-to-Target Mapping

### Families and Crates

- `block-amp-head` stays `amp_head`
- `block-amp-combo` stays `amp_combo`
- `block-cab` stays `cab`
- `block-full-rig` stays `full_rig`
- `block-nam` stays `nam`
- `block-delay` stays `delay`
- `block-pitch` stays `pitch`
- `block-reverb` stays `reverb`
- `block-routing` stays `routing`
- `block-gain` stays crate-local for now, but public family remains `drive`
- `block-dyn` stays crate-local for now, but public family becomes `dynamics`
- `block-mod` stays crate-local for now, but public family becomes `modulation`
- `block-util` stays crate-local for now, but public family becomes `utility`
- `audio-ir` is renamed to `ir`
- new crate `block-ir` is added for user-imported IR blocks
- new crate `block-wah` is added for wah models

### Models Moved Under Families

- `compressor` becomes a model under `dynamics`
- `gate` becomes a model under `dynamics`
- `tremolo` becomes a model under `modulation`
- `tuner` becomes a model under `utility`
- `eq` remains a model under `filter`

## Contract Changes

### Public `effect_type`

The public contract must expose family-level values only.

Examples:

- `dynamics`
- `modulation`
- `utility`
- `filter`
- `wah`
- `ir`

The contract must stop exposing model-like values such as:

- `compressor`
- `gate`
- `tremolo`
- `tuner`

### `model`

`model` remains the concrete implementation identifier inside a family.

Examples:

- `effect_type: dynamics`, `model: compressor_studio_clean`
- `effect_type: dynamics`, `model: gate_basic`
- `effect_type: modulation`, `model: tremolo_sine`
- `effect_type: utility`, `model: tuner_chromatic`
- `effect_type: wah`, `model: cry_wah_classic`
- `effect_type: ir`, `model: generic_ir`

## GUI Taxonomy

The GUI should stop correcting the core taxonomy and only translate family names into short labels.

Recommended labels:

- `amp_head` -> `AMP`
- `amp_combo` -> `COMBO`
- `cab` -> `CAB`
- `ir` -> `IR`
- `full_rig` -> `RIG`
- `nam` -> `NAM`
- `drive` -> `DRIVE`
- `dynamics` -> `DYN`
- `filter` -> `FILTER`
- `wah` -> `WAH`
- `modulation` -> `MOD`
- `delay` -> `DLY`
- `reverb` -> `RVB`
- `pitch` -> `PITCH`
- `utility` -> `UTIL`
- `routing` -> `LOOP`

This keeps the GUI dynamic while removing hardcoded model-to-family corrections.

## Implementation Scope

### Core

- rename crate `audio-ir` to `ir`
- create crate `block-ir`
- create crate `block-wah`
- update `project` contract to expose family-level `effect_type`
- update family registries and schema resolution to return family-level `effect_type`
- keep existing model auto-registration direction

### YAML

- update serialization and deserialization to read and write the new family names
- no backward-compatibility aliasing in this migration

### Application and Runtime

- validation must operate on the new family-level taxonomy
- runtime dispatch must resolve by family plus model, never by model masquerading as family

### GUI

- block type picker must list the new families
- short labels must be derived from family values only
- remove remaps that translate model-like effect types into family labels
- add `IR` and `WAH` as first-class block categories

## Error Handling

- unknown family: reject at schema/validation level
- unknown model within a known family: ignore on load where the current project policy requires ignore-on-missing, but do not let that mutate the family taxonomy
- GUI must not assume all families have models available; empty families should render as unavailable instead of crashing

## Testing

### Unit

- contract tests for family-level `effect_type`
- schema tests for `dynamics`, `modulation`, `utility`, `filter`, `wah`, and `ir`
- GUI helper tests for family-to-label mapping
- YAML roundtrip tests using the new family names

### Integration

- project load with `IR` and `WAH`
- runtime build for `dynamics` models, `modulation` models, and `utility` models under the new family taxonomy

### Regression

- assert that no user-facing path exposes `compressor`, `gate`, `tremolo`, or `tuner` as `effect_type`

## Migration Impact

This is a breaking taxonomy migration.

- existing YAML using old family leakage must be updated
- GUI assumptions about old `effect_type` strings must be updated
- tests that assert the old taxonomy must be rewritten

This spec intentionally does not provide aliases because the user explicitly rejected compatibility shims for this migration.

## Recommended Order

1. rename `audio-ir` to `ir`
2. add `block-ir`
3. add `block-wah`
4. normalize `project` contract to family-level `effect_type`
5. update validation and runtime dispatch
6. update GUI block-family handling
7. update YAML fixtures and user config files
