# LV2 Plugin Host Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Load LV2 plugins as OpenRig blocks, registered under their corresponding effect types (gain, delay, reverb, etc.) alongside native/NAM models.

**Architecture:** Two new crates following the NAM/IR pattern: `lv2` (hosting) wraps LV2 plugin loading/instantiation via `lilv` FFI, and `block-lv2` (blocks) reads `lv2_catalog.json` to register each plugin as a model. Unlike NAM/IR which use compile-time registries, LV2 uses a **dynamic runtime registry** loaded from JSON.

**Tech Stack:** Rust, lilv (C library via FFI), lv2 core headers, dlopen for plugin binaries

---

## File Structure

```
crates/
  lv2/                          ← NEW: LV2 hosting crate
    Cargo.toml
    build.rs                    ← Links lilv library
    src/
      lib.rs                    ← Public API
      host.rs                   ← LV2 world, plugin loading, instantiation
      processor.rs              ← Lv2Processor implementing MonoProcessor/StereoProcessor
      ports.rs                  ← Port discovery and mapping

  block-lv2/                    ← NEW: LV2 block registry crate
    Cargo.toml
    src/
      lib.rs                    ← Public API (supported_models, build, schema, visual)
      catalog.rs                ← Reads lv2_catalog.json, builds dynamic registry
      registry.rs               ← Lv2ModelDefinition, lookup by model_id

  project/src/catalog.rs        ← MODIFY: Add block-lv2 entries to block_registry()
  engine/src/runtime.rs         ← MODIFY: Wire block-lv2 builders (if needed)
  block-core/src/lib.rs         ← MODIFY: Add EFFECT_TYPE_LV2 constant (if separate type)
```

## Key Design Decisions

### Dynamic vs Static Registry

NAM/IR use compile-time registries (`build.rs` scans `.rs` files for `MODEL_DEFINITION`). LV2 can't do this because plugins are discovered at runtime from JSON + filesystem.

**Solution:** `block-lv2` exposes `supported_models()` that reads `lv2_catalog.json` once (lazy static) and returns model IDs dynamically. The `BlockRegistryEntry` function pointers work the same way — they just look up the catalog instead of a const array.

### Effect Type Registration

Each LV2 plugin has an `effect_type` in the catalog (gain, delay, reverb, etc.). Two approaches:

**Option A — Single "lv2" effect type:** All LV2 plugins under one `EFFECT_TYPE_LV2` category.

**Option B — Merge into existing types:** ChowCentaur appears in `gain` picker alongside TS9, Dragonfly appears in `reverb` alongside Plate Foundation.

**Chosen: Option A first, Option B later.** Start with a single "LV2" effect type to avoid coupling with existing block crates. Later, we can extend the catalog system to merge LV2 models into their native categories.

### Parameter Normalization

LV2 parameters with 0-1.0 ranges → convert to 0-100% (same pattern as native blocks). Parameters with specific units (Hz, ms, dB) keep natural ranges. This conversion happens in `block-lv2` when building the schema from catalog data.

---

## Task 1: Create `lv2` hosting crate — Cargo.toml and build.rs

**Files:**
- Create: `crates/lv2/Cargo.toml`
- Create: `crates/lv2/build.rs`
- Modify: `Cargo.toml` (workspace members)

- [ ] **Step 1: Create crate directory**

```bash
mkdir -p crates/lv2/src
```

- [ ] **Step 2: Write Cargo.toml**

```toml
[package]
name = "lv2"
version = "0.1.0"
edition = "2021"

[dependencies]
block-core = { path = "../block-core" }
anyhow = { workspace = true }
log = { workspace = true }

[build-dependencies]
pkg-config = "0.3"
```

- [ ] **Step 3: Write build.rs**

```rust
fn main() {
    // Try pkg-config first for lilv
    if let Ok(lib) = pkg_config::probe_library("lilv-0") {
        for path in &lib.link_paths {
            println!("cargo:rustc-link-search=native={}", path.display());
        }
        return;
    }
    // Fallback: assume lilv is in system paths
    println!("cargo:rustc-link-lib=dylib=lilv-0");
}
```

- [ ] **Step 4: Create minimal src/lib.rs**

```rust
pub mod host;
pub mod ports;
pub mod processor;
```

- [ ] **Step 5: Add to workspace Cargo.toml**

Add `"crates/lv2"` to workspace members.

- [ ] **Step 6: Commit**

```
feat(lv2): create lv2 hosting crate skeleton
```

---

## Task 2: Implement LV2 host — plugin loading and instantiation

**Files:**
- Create: `crates/lv2/src/host.rs`

- [ ] **Step 1: Write Lv2Host struct**

Wraps lilv World, handles plugin discovery and instantiation. Uses raw FFI to lilv C API:
- `Lv2Host::new()` — creates lilv World
- `Lv2Host::load_plugin(uri, sample_rate)` — finds and instantiates a plugin
- `Drop` — cleans up World

- [ ] **Step 2: Write lilv FFI bindings**

Minimal bindings for: `lilv_world_new`, `lilv_world_load_all`, `lilv_world_get_all_plugins`, `lilv_plugins_get_by_uri`, `lilv_plugin_instantiate`, `lilv_instance_connect_port`, `lilv_instance_activate`, `lilv_instance_run`, `lilv_instance_deactivate`, `lilv_instance_free`.

- [ ] **Step 3: Commit**

```
feat(lv2): implement LV2 host with lilv FFI bindings
```

---

## Task 3: Implement LV2 port discovery and mapping

**Files:**
- Create: `crates/lv2/src/ports.rs`

- [ ] **Step 1: Write port types**

```rust
pub enum Lv2PortKind {
    AudioInput(usize),   // port index
    AudioOutput(usize),
    ControlInput(usize),
    ControlOutput(usize),
}

pub struct Lv2PortMap {
    pub audio_inputs: Vec<usize>,
    pub audio_outputs: Vec<usize>,
    pub control_inputs: Vec<ControlPort>,
    pub control_outputs: Vec<usize>,
}

pub struct ControlPort {
    pub index: usize,
    pub symbol: String,
    pub name: String,
    pub default: f32,
    pub min: f32,
    pub max: f32,
}
```

- [ ] **Step 2: Implement port discovery from lilv**

Query plugin ports via `lilv_plugin_get_num_ports`, `lilv_plugin_get_port_by_index`, classify by type (audio/control, input/output).

- [ ] **Step 3: Commit**

```
feat(lv2): implement LV2 port discovery and mapping
```

---

## Task 4: Implement Lv2Processor

**Files:**
- Create: `crates/lv2/src/processor.rs`
- Modify: `crates/lv2/src/lib.rs`

- [ ] **Step 1: Write Lv2Processor struct**

```rust
pub struct Lv2Processor {
    instance: *mut LilvInstance,
    port_map: Lv2PortMap,
    control_values: Vec<f32>,   // buffer for control port values
    audio_in: Vec<f32>,         // scratch buffer
    audio_out: Vec<f32>,        // scratch buffer
}
```

- [ ] **Step 2: Implement MonoProcessor for Lv2Processor**

For mono plugins (audio_in=1, audio_out=1): connect ports, run, return output.

- [ ] **Step 3: Implement StereoProcessor wrapper**

For stereo plugins (audio_in=2, audio_out=2): process frame as 2-sample block.

- [ ] **Step 4: Write public API in lib.rs**

```rust
pub fn load_lv2_plugin(
    plugin_dir: &str,
    uri: &str,
    sample_rate: f32,
    control_values: &[(String, f32)],
) -> Result<BlockProcessor>
```

- [ ] **Step 5: Commit**

```
feat(lv2): implement Lv2Processor with mono/stereo support
```

---

## Task 5: Create `block-lv2` crate — catalog and registry

**Files:**
- Create: `crates/block-lv2/Cargo.toml`
- Create: `crates/block-lv2/src/lib.rs`
- Create: `crates/block-lv2/src/catalog.rs`
- Create: `crates/block-lv2/src/registry.rs`
- Modify: `Cargo.toml` (workspace members)

- [ ] **Step 1: Create crate directory and Cargo.toml**

```toml
[package]
name = "block-lv2"
version = "0.1.0"
edition = "2021"

[dependencies]
lv2 = { path = "../lv2" }
block-core = { path = "../block-core" }
anyhow = { workspace = true }
serde = { workspace = true }
serde_json = "1"
log = { workspace = true }
once_cell = "1"
```

- [ ] **Step 2: Write catalog.rs — load lv2_catalog.json**

```rust
use once_cell::sync::Lazy;
use serde::Deserialize;

#[derive(Deserialize, Clone)]
pub struct Lv2CatalogPlugin {
    pub id: String,
    pub name: String,
    pub uri: String,
    pub plugin_dir: String,
    pub binary: String,
    pub effect_type: String,
    pub audio_in: usize,
    pub audio_out: usize,
    pub parameters: Vec<Lv2CatalogParameter>,
}

#[derive(Deserialize, Clone)]
pub struct Lv2CatalogParameter {
    pub symbol: String,
    pub name: String,
    pub default: f32,
    pub min: f32,
    pub max: f32,
    #[serde(rename = "type")]
    pub param_type: String,
    pub options: Option<Vec<Lv2CatalogEnumOption>>,
}

#[derive(Deserialize, Clone)]
pub struct Lv2CatalogEnumOption {
    pub label: String,
    pub value: f32,
}

static CATALOG: Lazy<Vec<Lv2CatalogPlugin>> = Lazy::new(|| {
    load_catalog().unwrap_or_default()
});

fn load_catalog() -> Result<Vec<Lv2CatalogPlugin>> {
    // Load from embedded or filesystem
    let json = include_str!("../../../assets/lv2_catalog.json");
    let data: serde_json::Value = serde_json::from_str(json)?;
    let plugins: Vec<Lv2CatalogPlugin> = serde_json::from_value(data["lv2_plugins"].clone())?;
    Ok(plugins)
}

pub fn all_plugins() -> &'static [Lv2CatalogPlugin] { &CATALOG }
pub fn find_plugin(id: &str) -> Option<&'static Lv2CatalogPlugin> {
    CATALOG.iter().find(|p| p.id == id)
}
```

- [ ] **Step 3: Write registry.rs — model definitions**

Dynamic registry that builds `ModelParameterSchema` from catalog data. Converts 0-1.0 params to 0-100%.

- [ ] **Step 4: Write lib.rs — public API**

```rust
pub fn supported_models() -> &'static [&'static str]
pub fn lv2_model_visual(model_id: &str) -> Option<ModelVisualData>
pub fn lv2_model_schema(model: &str) -> Result<ModelParameterSchema>
pub fn build_lv2_processor_for_layout(model: &str, params: &ParameterSet, sample_rate: f32, layout: AudioChannelLayout) -> Result<BlockProcessor>
```

- [ ] **Step 5: Add to workspace Cargo.toml**

- [ ] **Step 6: Commit**

```
feat(block-lv2): create block-lv2 crate with catalog and dynamic registry
```

---

## Task 6: Register block-lv2 in project catalog

**Files:**
- Modify: `crates/project/Cargo.toml` — add `block-lv2` dependency
- Modify: `crates/project/src/catalog.rs` — add LV2 to `block_registry()`
- Modify: `crates/block-core/src/lib.rs` — add `EFFECT_TYPE_LV2` constant (if needed)

- [ ] **Step 1: Add EFFECT_TYPE_LV2 constant**

```rust
pub const EFFECT_TYPE_LV2: &str = "lv2";
```

- [ ] **Step 2: Add BlockRegistryEntry for LV2**

```rust
BlockRegistryEntry {
    effect_type: EFFECT_TYPE_LV2,
    display_label: "LV2",
    icon_kind: "lv2",
    use_panel_editor: false,
    supported_models: block_lv2::supported_models,
    model_visual: block_lv2::lv2_model_visual,
},
```

Update array size from 16 to 17.

- [ ] **Step 3: Wire engine builder (if separate effect type)**

In `crates/engine/src/runtime.rs`, add match arm:
```rust
EFFECT_TYPE_LV2 => block_lv2::build_lv2_processor_for_layout(model, params, sample_rate, layout)
```

- [ ] **Step 4: cargo check**

- [ ] **Step 5: Commit**

```
feat: register block-lv2 in project catalog and engine
```

---

## Task 7: Test end-to-end with a simple LV2 plugin

**Files:**
- Modify: `project.yaml` or create test project

- [ ] **Step 1: Test loading ChowCentaur**

Add to a chain in project.yaml:
```yaml
- type: lv2
  model: chowcentaur
  enabled: true
  params:
    level: 50.0
    treble: 50.0
```

- [ ] **Step 2: Run and verify audio passes through**

```bash
RUST_LOG=info cargo run --bin adapter-gui
```

- [ ] **Step 3: Test with a stereo plugin (Dragonfly Hall)**

- [ ] **Step 4: Verify parameter changes in UI affect audio**

- [ ] **Step 5: Commit**

```
test: verify LV2 plugin loading with ChowCentaur and Dragonfly
```

---

## Dependencies

- **lilv** must be installed: `brew install lilv` (macOS) or `apt install liblilv-dev` (Linux)
- LV2 plugin binaries in `plugins/` directory
- `lv2_catalog.json` in `assets/`

## Risk: lilv availability

If lilv is not available on all platforms, we may need to:
1. Bundle lilv as a submodule and build from source (like we do with NAM)
2. Or implement a minimal LV2 host without lilv (just dlopen + LV2 core headers)

Option 2 is simpler since we already have the catalog JSON with all port metadata — we don't need lilv's discovery, just the loading/instantiation.
