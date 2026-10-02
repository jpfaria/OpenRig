# Creating Blocks and Models

## Overview

In OpenRig, each audio processor is a **block** with one or more **models**. This guide covers **native** models: DSP written in Rust inside a block crate (`block-preamp`, `block-delay`, …).

NAM captures, impulse responses, LV2 and VST3 plugins are not written here: they are packages (a `manifest.yaml` plus their files) in [OpenRig-plugins](https://github.com/jpfaria/OpenRig-plugins), loaded at runtime by `crates/plugin-loader`. To add one of those, follow that repository's docs.

## How Auto-Registration Works

Each block crate has a `build.rs` that scans the `.rs` files in `src/` for `pub const MODEL_DEFINITION` and generates `generated_registry.rs`, the list of every model in the crate. Adding a model = adding a file; there is no list to edit.

## Step-by-Step: Creating a New Model

### Step 1: Choose the Right Crate

| Effect type | Crate |
|---|---|
| Preamp | `crates/block-preamp/` |
| Full amplifier | `crates/block-amp/` |
| Cabinet | `crates/block-cab/` |
| Gain (overdrive, distortion, fuzz, boost, volume) | `crates/block-gain/` |
| Delay | `crates/block-delay/` |
| Reverb | `crates/block-reverb/` |
| Modulation (chorus, flanger, phaser, tremolo) | `crates/block-mod/` |
| Dynamics (compressor, gate, limiter) | `crates/block-dyn/` |
| Filter / EQ | `crates/block-filter/` |
| Wah | `crates/block-wah/` |
| Pitch | `crates/block-pitch/` |

### Step 2: Create the Model File

Create `src/native_<name>.rs` in that crate, opening with its responsibility line (see [file organization](file-organization.md)):

```rust
//! Responsibility: implements the spring reverb model.
```

Its tests go in a sibling `src/native_<name>_tests.rs`, wired at the end of the model file — never an inline test module:

```rust
#[cfg(test)]
#[path = "native_spring_reverb_tests.rs"]
mod tests;
```

### Step 3: Define MODEL_DEFINITION

The struct is in the crate's `registry.rs` and its fields differ a little per crate: preamp, amp, cab and gain also take `validate` and `asset_summary`; wah takes `validate`. A reverb looks like this:

```rust
pub const MODEL_ID: &str = "spring_reverb";
pub const DISPLAY_NAME: &str = "Spring Reverb";

pub const MODEL_DEFINITION: ReverbModelDefinition = ReverbModelDefinition {
    id: MODEL_ID,
    display_name: DISPLAY_NAME,
    brand: block_core::BRAND_NATIVE,
    backend_kind: ReverbBackendKind::Native,
    schema,
    build,
    supported_instruments: block_core::ALL_INSTRUMENTS,
    knob_layout: &[],
};
```

### Step 4: Implement the Functions

- **`schema()`** — returns the `ModelParameterSchema`: every parameter with its name, label, range, default, step and unit (`float_parameter`, `bool_parameter`, …).
- **`build(params, sample_rate, layout)`** — returns the `BlockProcessor` that runs on the audio thread: a `MonoProcessor` or `StereoProcessor` for the given layout. No allocation, lock or I/O inside `process`.
- **`validate(params)`** and **`asset_summary(params)`**, where the crate's struct has them — reject out-of-range values; return a short text of the current settings.

### Step 5: Test, then Build

Write the tests first and watch them fail (red-first, see [testing](../testing.md)), then implement and run only that crate's tests:

```bash
cargo test -p block-reverb native_spring_reverb
```

`build.rs` picks the new model up by itself. The build must have zero warnings.

## Naming

| Field | Rule | Example |
|---|---|---|
| `id` | snake_case; it is what projects and presets save, so it never changes once shipped | `spring_reverb` |
| `display_name` | Human-readable, no brand | "Spring Reverb" |
| `brand` | Always `block_core::BRAND_NATIVE` for a model in this repository | `"native"` |
| File name | `native_` + the model's name | `native_spring_reverb.rs` |

## Visuals (optional)

A native model renders with the generic block panel. It can add:

- **Knob positions** — `knob_layout`, a list of `block_core::KnobLayoutEntry` (`param_key`, `svg_cx`, `svg_cy`, `svg_r`, `min`, `max`, `step`) placing each knob on the panel.
- **Colours and font** — an entry in the crate's `model_visual.rs` (`ModelColorOverride`: panel background, panel text, brand strip background, model font).
- **Panel artwork** — `assets/models/<id>.svg`, wired in `crates/adapter-gui/ui/components/block_panel_brand_strip.slint`.
- **Description** — the model's entry under `plugins:` in `assets/blocks/metadata/en-US.yaml` and `pt-BR.yaml`.

### Brand logos

Logos are for the brands of plugin models; native models have none.

- One per brand id: `assets/brands/<brand>/logo.svg` (or `.png`), single colour on a transparent background, tinted in the UI. Only Vox keeps its colours.
- Every brand folder is mapped in `crates/adapter-gui/ui/components/brand_logo.slint`; a test fails when one is missing.
- Source logos from cdn.worldvectorlogo.com and remove the background.

## Code Quality Rules

- **Zero warnings** — `cargo build` must produce no warnings.
- **Zero coupling** — a model never references other models, brands or specific effect types. Use the abstractions in `block-core`.
- **Single source of truth** — constants are defined once.
- **Supported instruments** — declare them with the `block-core` constants (`ALL_INSTRUMENTS`, `GUITAR_BASS`, `GUITAR_ACOUSTIC_BASS`, …).

## Testing

Test that the model processes audio correctly:

1. Build the processor with known parameters.
2. Feed it a test signal (silence, sine, impulse).
3. Check the output is within the expected range.
4. Cover the edges: minimum and maximum parameter values, mono and stereo layouts.

The existing `native_*_tests.rs` files in each block crate are the reference.
