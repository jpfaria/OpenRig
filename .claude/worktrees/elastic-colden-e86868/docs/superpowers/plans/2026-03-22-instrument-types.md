# Instrument Types Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Each chain has an instrument type. When adding blocks, only compatible models appear. Models define which instruments they support.

**Architecture:** `supported_instruments` field added to ModelVisualData (block-core) and each ModelDefinition. Chain gets `instrument` field persisted in YAML. Catalog propagates instrument data to adapter-gui. Adapter-gui filters block/model pickers by chain instrument. "generic" shows everything.

**Tech Stack:** Rust, Slint

**Spec:** `docs/superpowers/specs/2026-03-22-instrument-types-design.md`

---

## Files to modify

| Layer | File | Change |
|-------|------|--------|
| core | `crates/block-core/src/lib.rs` | Add `supported_instruments` to `ModelVisualData` |
| models | All 30+ model `.rs` files | Add `supported_instruments` to each `MODEL_DEFINITION` |
| models | All 14 `registry.rs` files | Add `supported_instruments` to each `XxxModelDefinition` struct |
| models | All 14 `lib.rs` crate files | Return `supported_instruments` in `xxx_model_visual()` |
| catalog | `crates/project/src/catalog.rs` | Add `supported_instruments` to `BlockModelCatalogEntry`, add default per type |
| chain | `crates/project/src/chain.rs` | Add `instrument: String` to `Chain` |
| yaml | `crates/infra-yaml/src/lib.rs` | Serialize/deserialize `instrument` in chain YAML |
| gui | `crates/adapter-gui/src/lib.rs` | Filter pickers by instrument, pass instrument to UI |
| gui | `crates/adapter-gui/ui/models.slint` | Add `instrument` to `ProjectChainItem` |
| gui | `crates/adapter-gui/ui/pages/project_chains.slint` | Show instrument icon in ChainRow |
| gui | `crates/adapter-gui/ui/assets/` | Instrument icon SVGs |
| docs | `CLAUDE.md` | Document instrument types |

---

### Task 1: Add supported_instruments to block-core and ModelVisualData

**Files:**
- Modify: `crates/block-core/src/lib.rs`

- [ ] **Step 1: Add field to ModelVisualData**

```rust
pub struct ModelVisualData {
    pub brand: &'static str,
    pub type_label: &'static str,
    pub supported_instruments: &'static [&'static str],
}
```

- [ ] **Step 2: Build**
```bash
cargo build -p block-core
```
This will fail because all crates constructing ModelVisualData need the new field. That's expected — Task 2 fixes it.

- [ ] **Step 3: Commit**
```bash
git commit -m "feat: add supported_instruments to ModelVisualData"
```

---

### Task 2: Add supported_instruments to ALL ModelDefinition structs and model files

**Files:**
- Modify: All 14 `registry.rs` files (block-preamp, block-amp, block-cab, block-gain, block-delay, block-reverb, block-dyn, block-filter, block-wah, block-mod, block-pitch, block-ir, block-nam, block-util, block-full-rig)
- Modify: All 30+ model `.rs` files
- Modify: All 14 `lib.rs` crate files

- [ ] **Step 1: Add field to each XxxModelDefinition struct**

In each `registry.rs`, add:
```rust
pub supported_instruments: &'static [&'static str],
```

- [ ] **Step 2: Define instrument constants**

Create constants in `block-core/src/lib.rs`:
```rust
pub const INST_ELECTRIC_GUITAR: &str = "electric_guitar";
pub const INST_ACOUSTIC_GUITAR: &str = "acoustic_guitar";
pub const INST_BASS: &str = "bass";
pub const INST_VOICE: &str = "voice";
pub const INST_KEYS: &str = "keys";
pub const INST_DRUMS: &str = "drums";

pub const ALL_INSTRUMENTS: &[&str] = &[
    INST_ELECTRIC_GUITAR, INST_ACOUSTIC_GUITAR, INST_BASS,
    INST_VOICE, INST_KEYS, INST_DRUMS,
];

pub const GUITAR_BASS: &[&str] = &[INST_ELECTRIC_GUITAR, INST_BASS];
pub const GUITAR_ACOUSTIC_BASS: &[&str] = &[INST_ELECTRIC_GUITAR, INST_ACOUSTIC_GUITAR, INST_BASS];
```

- [ ] **Step 3: Add supported_instruments to each MODEL_DEFINITION**

Use the constants. Examples:
- Marshall JCM 800 (preamp NAM): `supported_instruments: GUITAR_BASS`
- American Clean (preamp native): `supported_instruments: GUITAR_BASS`
- Plate Foundation (reverb): `supported_instruments: ALL_INSTRUMENTS`
- Blues Driver BD-2 (gain NAM): `supported_instruments: GUITAR_BASS`

- [ ] **Step 4: Update xxx_model_visual() in each lib.rs**

Return `supported_instruments` from the ModelDefinition:
```rust
pub fn preamp_model_visual(model_id: &str) -> Option<ModelVisualData> {
    let def = registry::find_model_definition(model_id).ok()?;
    Some(ModelVisualData {
        brand: def.brand,
        type_label: match def.backend_kind { ... },
        supported_instruments: def.supported_instruments,
    })
}
```

- [ ] **Step 5: Build all block crates**
```bash
cargo build
```

- [ ] **Step 6: Commit**
```bash
git commit -m "feat: all models define supported_instruments"
```

---

### Task 3: Add instrument to Chain and YAML

**Files:**
- Modify: `crates/project/src/chain.rs`
- Modify: `crates/infra-yaml/src/lib.rs`

- [ ] **Step 1: Add instrument field to Chain**

```rust
pub struct Chain {
    pub instrument: String, // "electric_guitar", "generic", etc.
    // ... existing fields
}
```

- [ ] **Step 2: Add instrument to ChainYaml**

```rust
struct ChainYaml {
    #[serde(default = "default_instrument")]
    instrument: String,
    // ... existing fields
}

fn default_instrument() -> String {
    "electric_guitar".to_string()
}
```

- [ ] **Step 3: Wire up into_chain and from_chain**

```rust
// into_chain:
instrument: self.instrument,

// from_chain:
instrument: chain.instrument.clone(),
```

- [ ] **Step 4: Build and test**
```bash
cargo build
cargo test -p infra-yaml
```

- [ ] **Step 5: Commit**
```bash
git commit -m "feat: chain has instrument field, persisted in YAML"
```

---

### Task 4: Propagate supported_instruments through catalog

**Files:**
- Modify: `crates/project/src/catalog.rs`

- [ ] **Step 1: Add supported_instruments to BlockModelCatalogEntry**

```rust
pub struct BlockModelCatalogEntry {
    pub supported_instruments: Vec<String>,
    // ... existing fields
}
```

- [ ] **Step 2: Populate in supported_block_models()**

Read from `model_visual` function and populate:
```rust
supported_instruments: visual.as_ref()
    .map(|v| v.supported_instruments.iter().map(|s| s.to_string()).collect())
    .unwrap_or_else(|| vec!["electric_guitar".into(), "acoustic_guitar".into(), "bass".into(), "voice".into(), "keys".into(), "drums".into()]),
```

- [ ] **Step 3: Build and test**
```bash
cargo build
cargo test -p project
```

- [ ] **Step 4: Commit**
```bash
git commit -m "feat: catalog propagates supported_instruments"
```

---

### Task 5: Filter block/model pickers by instrument in adapter-gui

**Files:**
- Modify: `crates/adapter-gui/src/lib.rs`

- [ ] **Step 1: Add instrument parameter to block_model_picker_items**

Change signature to accept instrument:
```rust
fn block_model_picker_items(effect_type: &str, instrument: &str) -> Vec<BlockModelPickerItem>
```

Filter models:
```rust
.filter(|item| instrument == "generic" || item.supported_instruments.iter().any(|i| i == instrument))
```

- [ ] **Step 2: Add instrument parameter to block_type_picker_items**

Filter types that have at least one compatible model:
```rust
fn block_type_picker_items(instrument: &str) -> Vec<BlockTypePickerItem>
```

Only include types where `block_model_picker_items(effect_type, instrument)` is non-empty.

- [ ] **Step 3: Update all callers**

Pass `chain.instrument` (or draft instrument) to these functions wherever they're called.

- [ ] **Step 4: Build**
```bash
cargo build
```

- [ ] **Step 5: Commit**
```bash
git commit -m "feat: filter block/model pickers by chain instrument"
```

---

### Task 6: UI — instrument icon in ChainRow + instrument selector in chain editor

**Files:**
- Modify: `crates/adapter-gui/ui/models.slint`
- Modify: `crates/adapter-gui/ui/pages/project_chains.slint`
- Create: `crates/adapter-gui/ui/assets/instruments/` (7 SVG icons)
- Modify: `crates/adapter-gui/src/lib.rs`

- [ ] **Step 1: Add instrument to ProjectChainItem**

In `models.slint`:
```slint
export struct ProjectChainItem {
    instrument: string,
    // ... existing fields
}
```

- [ ] **Step 2: Populate instrument in replace_project_chains**

In `lib.rs`, set `instrument: chain.instrument.clone().into()`.

- [ ] **Step 3: Create instrument icon SVGs**

Create 7 SVGs in `crates/adapter-gui/ui/assets/instruments/`:
- `electric_guitar.svg`
- `acoustic_guitar.svg`
- `bass.svg`
- `voice.svg`
- `keys.svg`
- `drums.svg`
- `generic.svg`

- [ ] **Step 4: Show instrument icon in ChainRow**

In `project_chains.slint`, add instrument icon next to the chain title:
```slint
Image {
    x: 56px;
    y: 16px;
    width: 16px;
    height: 16px;
    source: /* ternary chain by instrument */;
    colorize: #8090a0;
}
```

Shift the chain title to make room.

- [ ] **Step 5: Add instrument selector to chain editor**

In the chain configuration window, add instrument selection (dropdown or segmented control). Only shown when creating a new chain. Disabled when editing existing chain.

- [ ] **Step 6: Build and test**
```bash
cargo build
```

- [ ] **Step 7: Commit**
```bash
git commit -m "feat: instrument icon in chain, selector in chain editor"
```

---

### Task 7: Update CLAUDE.md and existing YAML files

**Files:**
- Modify: `CLAUDE.md`
- Modify: `project.yaml` (add instrument to existing chains)
- Modify: `~/.openrig/project.yaml` (add instrument)

- [ ] **Step 1: Update CLAUDE.md**

Add section about instrument types, how models define compatibility, and how filtering works.

- [ ] **Step 2: Update YAML files**

Add `instrument: electric_guitar` to existing chains that don't have it (backwards compat default handles this, but be explicit).

- [ ] **Step 3: Commit**
```bash
git commit -m "docs: update CLAUDE.md with instrument types"
```

---

## Verification

1. Open app — existing chains show "electric_guitar" icon
2. Create new chain — instrument selector appears, choose "voice"
3. Add block to voice chain — only delay, reverb, dynamics, filter, mod, pitch, IR, utility appear (no preamp, amp, cab, gain, wah)
4. Create "generic" chain — all block types appear
5. Save and reload — instrument persists
6. Existing YAML without instrument field loads with default "electric_guitar"
