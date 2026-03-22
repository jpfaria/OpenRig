# Instrument Types — Design Spec

## Problem

When creating a chain, all block types and models appear regardless of context. A vocalist doesn't need to see guitar preamps, and a guitarist doesn't need vocal processors. This makes block selection noisy and confusing.

## Solution

Each chain has an instrument type (electric guitar, acoustic guitar, bass, voice, keys, drums, generic). Each model defines which instruments it supports. When adding blocks to a chain, only compatible models appear. Choosing "generic" shows everything.

## Instruments

| ID | Label | Icon |
|---|---|---|
| `electric_guitar` | Electric Guitar | electric guitar |
| `acoustic_guitar` | Acoustic Guitar | acoustic guitar |
| `bass` | Bass | bass guitar |
| `voice` | Voice | microphone |
| `keys` | Keys | keyboard |
| `drums` | Drums | drums |
| `generic` | Generic | generic/all |

More instruments can be added in the future without architectural changes.

## Data Model

### Model defines compatibility

Each model's `ModelDefinition` gets a new field:
```rust
pub supported_instruments: &'static [&'static str],
```

Examples:
- `marshall_jcm_800_2203` (preamp NAM) → `&["electric_guitar", "bass"]`
- `plate_foundation` (reverb) → `&["electric_guitar", "acoustic_guitar", "bass", "voice", "keys", "drums"]` (all except generic — generic shows everything by default)
- `american_2x12` (cab) → `&["electric_guitar", "bass"]`
- Future `acoustic_martin_d28` (cab) → `&["acoustic_guitar"]`

### Default per block type

If a model doesn't specify, it inherits from the block type default:

| Block Type | Default instruments |
|---|---|
| preamp | electric_guitar, acoustic_guitar, bass |
| amp | electric_guitar, acoustic_guitar, bass |
| cab | electric_guitar, bass |
| full_rig | electric_guitar, bass |
| gain | electric_guitar, bass |
| wah | electric_guitar, bass |
| nam | electric_guitar, bass |
| delay, reverb, dynamics, filter, mod, pitch, ir, utility | all instruments |

### Chain stores instrument

```rust
pub struct Chain {
    pub instrument: String, // "electric_guitar", "voice", etc.
    // ... existing fields
}
```

Persisted in YAML:
```yaml
chains:
  - instrument: electric_guitar
    description: guitar 1
    blocks: [...]
```

## Filtering Logic

When adding a block to a chain:

```
if chain.instrument == "generic" {
    show ALL types and models
} else {
    show only models where:
        model.supported_instruments.contains(chain.instrument)
    show only types that have at least one compatible model
}
```

## UI

### Creating a chain

In the existing chain configuration window (ChainEditorPage), add an instrument selector. This is shown when creating a new chain. Once set, it cannot be changed.

### ChainRow

Show the instrument icon next to the chain name (left side, near the power toggle).

### Block type picker

Filtered by chain instrument. Types with zero compatible models are hidden.

### Model picker

Filtered by chain instrument. Only models that support the chain's instrument appear.

## Propagation through catalog

`BlockModelCatalogEntry` gets `supported_instruments: Vec<String>`.

`supported_block_models(effect_type)` returns all models. The adapter-gui filters by instrument when building the picker items.

`ModelVisualData` in block-core gets `supported_instruments: &'static [&'static str]`.

Each crate's `xxx_model_visual()` function returns this from the ModelDefinition.

## What does NOT change

- Audio processing — instrument is metadata only, no DSP difference
- Block behavior — blocks work the same regardless of chain instrument
- Existing chains — default to "electric_guitar" for backwards compatibility
- Runtime — no changes to CPAL, engine, or audio pipeline

## Future extensibility

- Add new instruments: just add an ID, label, and icon. No code changes needed beyond the instrument list.
- Models can be updated to support new instruments without changing architecture.
- Per-model overrides work naturally — a future "bass cab" model just lists `["bass"]` in its supported_instruments.
