# Creating Blocks and Models

## Overview

In OpenRig, each audio processor is a **block** with one or more **models**. Blocks are organized in dedicated crates (e.g., `block-preamp`, `block-delay`). Models are discovered automatically at build time -- you create a Rust file with a `MODEL_DEFINITION` constant, and the build system registers it. No manual wiring required.

## How Auto-Registration Works

Each block crate has a `build.rs` that scans all `.rs` files in `src/` looking for `pub const MODEL_DEFINITION` declarations. It generates a `generated_registry.rs` file containing an array of all discovered model definitions. This means:

1. No manual registration needed.
2. No central registry file to maintain.
3. Adding a model = creating a file.

## Step-by-Step: Creating a New Model

### Step 1: Choose the Right Crate

Pick the block crate that matches your effect type:

| Effect Type | Crate |
|---|---|
| Pre-amplifier | `crates/block-preamp/` |
| Full amplifier | `crates/block-amp/` |
| Cabinet/speaker | `crates/block-cab/` |
| Gain/overdrive/distortion | `crates/block-gain/` |
| Delay | `crates/block-delay/` |
| Reverb | `crates/block-reverb/` |
| Modulation (chorus, tremolo) | `crates/block-mod/` |
| Dynamics (compressor, gate) | `crates/block-dyn/` |
| Filter/EQ | `crates/block-filter/` |
| Wah | `crates/block-wah/` |
| Utility (tuner, etc.) | `crates/block-util/` |
| Acoustic body | `crates/block-body/` |

### Step 2: Create the Model File

Create a new `.rs` file in the crate's `src/` directory. Use the naming convention:

- `native_` prefix for Native DSP models
- `nam_` prefix for NAM-captured models
- `ir_` prefix for IR-based models
- `lv2_` prefix for LV2 plugin wrappers

Example: `src/native_spring_reverb.rs`

### Step 3: Define MODEL_DEFINITION

Every model file must export a `pub const MODEL_DEFINITION` constant. The exact type depends on the block crate (e.g., `ReverbModelDefinition`, `PreampModelDefinition`), but the structure is consistent:

```rust
use block_core::{AudioChannelLayout, BlockProcessor, ParameterSet};
use anyhow::Result;

pub const MODEL_DEFINITION: ReverbModelDefinition = ReverbModelDefinition {
    id: "spring_reverb",
    display_name: "Spring Reverb",
    brand: "native",
    backend_kind: ReverbBackendKind::Native,
    supported_instruments: &["electric_guitar", "acoustic_guitar", "bass", "voice", "keys", "drums"],
    schema: || Ok(model_schema()),
    validate: |params| validate_params(params),
    asset_summary: |params| Ok(format!("mix={}%", params.get_f32("mix").unwrap_or(50.0))),
    build: |params, sample_rate, layout| build_processor(params, sample_rate, layout),
};
```

### Step 4: Implement the Required Functions

Each model definition references four functions that you must implement in the same file:

- **`schema()`** -- Returns a `ModelParameterSchema` describing all parameters (name, display_name, min, max, default, step, unit).
- **`validate(params)`** -- Validates that parameter values are within acceptable bounds.
- **`asset_summary(params)`** -- Returns a short string summarizing current settings (used for thumbnails and compact views).
- **`build(params, sample_rate, layout)`** -- Constructs and returns a `BlockProcessor` that performs the actual audio processing.

### Step 5: Build and Verify

```bash
cargo build -p block-reverb
```

The `build.rs` automatically discovers your new model and adds it to the registry. Verify the build completes with zero warnings.

## Naming Conventions

| Field | Rule | Example |
|---|---|---|
| `id` | snake_case, prefixed by source | `native_spring_reverb`, `nam_mesa_mark_v` |
| `display_name` | Human-readable, NO brand in name | "Spring Reverb", "Mark V" |
| `brand` | Lowercase brand or "native" | `"mesa"`, `"marshall"`, `"native"` |
| File name | Same prefix convention as `id` | `native_spring_reverb.rs` |

## Assets

For models that need visual assets (amp panels, pedal images), the following files are required.

### controls.svg

The panel image for the Block Editor. Follow the AC30 reference pattern:

```svg
<svg viewBox="0 0 800 200" width="800" height="200">
  <!-- Dark gradient background -->
  <rect fill="url(#panel)"/>
  <!-- Model label (left) -->
  <!-- Section dividers (dashed lines) -->
  <!-- Section labels (top) -->
  <!-- Knob anchors: fill="#111" stroke="#505050" stroke-width="1.5" -->
  <!-- Parameter labels below each knob -->
</svg>
```

Guidelines for `controls.svg`:

- Editable controls get `id="ctrl-xxx"` for future overlay support.
- Non-editable controls get `opacity="0.6"` without an `id`.

### component.yaml

Defines asset paths and SVG knob positions:

```yaml
controls_svg: "path/to/controls.svg"
controls:
  - name: gain
    svg_cx: 150
    svg_cy: 100
  - name: bass
    svg_cx: 250
    svg_cy: 100
```

### Brand Logos

- Always source from cdn.worldvectorlogo.com.
- Remove backgrounds.
- Use `fill="currentColor"` for theming (except multi-color logos like Vox).

## Code Quality Rules

- **Zero warnings** -- `cargo build` must produce no warnings.
- **Zero coupling** -- Your model must not reference other models, brands, or specific effect types. Use abstractions from `block-core`.
- **Single source of truth** -- Constants are defined once, never duplicated.
- **Supported instruments** -- Always declare which instruments your model supports using the constants from `block-core` (e.g., `ALL_INSTRUMENTS`, `GUITAR_BASS`, `GUITAR_ACOUSTIC_BASS`).

## Testing

Test that your block processes audio correctly:

1. Create a `BlockProcessor` with known parameters.
2. Feed it a test buffer (silence, sine wave, impulse).
3. Verify output is within expected ranges.
4. Test edge cases (min/max parameter values, mono/stereo layouts).

```rust
#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_spring_reverb_processes_silence() {
        let params = ParameterSet::default();
        let processor = build_processor(&params, 44100.0, AudioChannelLayout::Mono).unwrap();
        let mut buffer = vec![0.0f32; 1024];
        processor.process(&mut buffer);
        // Silence in should produce silence (or near-silence) out
        assert!(buffer.iter().all(|&s| s.abs() < 1e-6));
    }
}
```
