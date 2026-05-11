# Block Thumbnails Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Replace plain 84x84 colored chips in the chain view with pedal/amp thumbnail images, using real images for known plugins and generic templates for native blocks.

**Architecture:** Create a new `crates/block-thumbnails` crate that embeds all thumbnail PNGs via `include_bytes!`. Each thumbnail is registered by `(effect_type, model_id)`. The adapter-gui passes thumbnail bytes as `slint::Image` to the Slint `BlockChip`. Fallback chain: model thumbnail → type default thumbnail → current icon.

**Tech Stack:** Rust (`include_bytes!`, `slint::Image::from_rgba8`), PNG decoding (`image` crate), Slint (`Image` property)

---

## File Structure

### New files
- `crates/block-thumbnails/Cargo.toml` — new crate
- `crates/block-thumbnails/src/lib.rs` — thumbnail registry: `fn thumbnail_for(effect_type, model_id) -> Option<&[u8]>`
- `crates/block-thumbnails/build.rs` — auto-discovers PNGs from `assets/blocks/thumbnails/`
- `assets/blocks/thumbnails/{effect_type}/{model_id}.png` — per-model thumbnails
- `assets/blocks/thumbnails/{effect_type}/_default.png` — fallback per type
- `scripts/import-thumbnails.sh` — one-time script to copy/resize thumbnails from `plugins/` into `assets/blocks/thumbnails/`

### Modified files
- `crates/adapter-gui/Cargo.toml` — add dep on `block-thumbnails`
- `crates/adapter-gui/src/lib.rs` — convert thumbnail bytes to `slint::Image`, pass to UI
- `crates/adapter-gui/ui/models.slint` — add `thumbnail: image` to `ChainBlockItem`
- `crates/adapter-gui/ui/pages/project_chains.slint` — render thumbnail image in BlockChip instead of icon

---

## Task 1: Create thumbnail asset structure and import script

**Files:**
- Create: `scripts/import-thumbnails.sh`
- Create: `assets/blocks/thumbnails/` directory structure

- [ ] **Step 1: Create the import script**

Script that:
1. Scans `plugins/*/modgui/thumbnail*.png`
2. Maps plugin names to effect_type/model_id (manual mapping table in script)
3. Resizes to max 128px height (keeping aspect ratio) using `sips` (macOS native)
4. Copies to `assets/blocks/thumbnails/{effect_type}/{model_id}.png`
5. For types without specific thumbnails, copies mod-resources generic templates as `_default.png`

- [ ] **Step 2: Run the import for known plugins**

Focus on plugins that map to our existing block types:
- `gx_jcm800pre` → `preamp/marshall_jcm_800_2203.png`
- GxPlugins pedals → `gain/*.png`, `preamp/*.png`
- DragonflyReverb → `reverb/dragonfly_hall.png`
- tap-delay → `delay/tap_echo.png`
- ZamComp → `dynamics/zam_comp.png`
- MVerb → `reverb/mverb.png`
- etc.

- [ ] **Step 3: Create generic _default.png per type**

Use mod-resources templates (japanese/boxy style) with type-specific colors:
- `preamp/_default.png` — orange japanese pedal
- `amp/_default.png` — british metallic head
- `cab/_default.png` — dark rack
- `gain/_default.png` — green japanese pedal
- `delay/_default.png` — blue boxy pedal
- `reverb/_default.png` — teal boxy pedal
- `dynamics/_default.png` — blue japanese pedal
- `filter/_default.png` — yellow japanese pedal
- `modulation/_default.png` — green boxy pedal
- `wah/_default.png` — green japanese pedal
- `pitch/_default.png` — purple boxy pedal
- `utility/_default.png` — gray boxy pedal
- `body/_default.png` — brown boxy pedal
- `ir/_default.png` — light blue boxy pedal
- `nam/_default.png` — magenta boxy pedal
- `full_rig/_default.png` — cyan boxy pedal

- [ ] **Step 4: Commit**

```
git add assets/blocks/thumbnails/ scripts/import-thumbnails.sh
git commit -m "Add block thumbnail assets and import script"
```

---

## Task 2: Create block-thumbnails crate

**Files:**
- Create: `crates/block-thumbnails/Cargo.toml`
- Create: `crates/block-thumbnails/src/lib.rs`
- Create: `crates/block-thumbnails/build.rs`

- [ ] **Step 1: Create Cargo.toml**

```toml
[package]
name = "block-thumbnails"
version.workspace = true
edition.workspace = true

[dependencies]
block-core = { path = "../block-core" }

[build-dependencies]
```

No external deps — just `include_bytes!` and a lookup function.

- [ ] **Step 2: Create build.rs**

Auto-discover all `.png` files in `assets/blocks/thumbnails/` and generate a registry module:

```rust
// build.rs scans assets/blocks/thumbnails/{type}/{name}.png
// Generates: const THUMBNAILS: &[(&str, &str, &[u8])] = &[
//     ("preamp", "marshall_jcm_800_2203", include_bytes!("../../assets/blocks/thumbnails/preamp/marshall_jcm_800_2203.png")),
//     ("preamp", "_default", include_bytes!("../../assets/blocks/thumbnails/preamp/_default.png")),
//     ...
// ];
```

- [ ] **Step 3: Create lib.rs**

```rust
include!(concat!(env!("OUT_DIR"), "/generated_thumbnails.rs"));

/// Returns the PNG bytes for a specific model, or the type default, or None.
pub fn thumbnail_png(effect_type: &str, model_id: &str) -> Option<&'static [u8]> {
    // Try exact match first
    THUMBNAILS.iter()
        .find(|(t, m, _)| *t == effect_type && *m == model_id)
        .map(|(_, _, bytes)| *bytes)
        .or_else(|| {
            // Fallback to type default
            THUMBNAILS.iter()
                .find(|(t, m, _)| *t == effect_type && *m == "_default")
                .map(|(_, _, bytes)| *bytes)
        })
}
```

- [ ] **Step 4: Verify it compiles**

```bash
cargo build -p block-thumbnails
```

- [ ] **Step 5: Commit**

```
git add crates/block-thumbnails/
git commit -m "Add block-thumbnails crate with auto-discovered embedded PNGs"
```

---

## Task 3: Integrate thumbnails into adapter-gui

**Files:**
- Modify: `crates/adapter-gui/Cargo.toml` — add `block-thumbnails` dep
- Modify: `crates/adapter-gui/ui/models.slint` — add `thumbnail: image` to ChainBlockItem
- Modify: `crates/adapter-gui/src/lib.rs` — decode PNG → slint::Image, set on ChainBlockItem

- [ ] **Step 1: Add dependency**

In `crates/adapter-gui/Cargo.toml`:
```toml
block-thumbnails = { path = "../block-thumbnails" }
image = { version = "0.25", default-features = false, features = ["png"] }
```

- [ ] **Step 2: Add thumbnail field to ChainBlockItem**

In `models.slint`:
```slint
export struct ChainBlockItem {
    kind: string,
    icon_kind: string,
    type_label: string,
    label: string,
    family: string,
    enabled: bool,
    thumbnail: image,       // NEW
    has_thumbnail: bool,    // NEW
}
```

- [ ] **Step 3: Build thumbnail image in chain_block_item_from_block**

In `lib.rs`, after constructing ChainBlockItem, decode the PNG and convert to slint::Image:

```rust
fn load_thumbnail_image(effect_type: &str, model_id: &str) -> (slint::Image, bool) {
    match block_thumbnails::thumbnail_png(effect_type, model_id) {
        Some(png_bytes) => {
            match image::load_from_memory_with_format(png_bytes, image::ImageFormat::Png) {
                Ok(img) => {
                    let rgba = img.to_rgba8();
                    let buffer = slint::SharedPixelBuffer::<slint::Rgba8Pixel>::clone_from_slice(
                        rgba.as_raw(),
                        rgba.width(),
                        rgba.height(),
                    );
                    (slint::Image::from_rgba8(buffer), true)
                }
                Err(_) => (slint::Image::default(), false)
            }
        }
        None => (slint::Image::default(), false)
    }
}
```

Call this in `chain_block_item_from_block` and set `.thumbnail` and `.has_thumbnail`.

- [ ] **Step 4: Verify it compiles**

```bash
cargo build -p adapter-gui
```

- [ ] **Step 5: Commit**

```
git add crates/adapter-gui/
git commit -m "Integrate block thumbnails into chain block items"
```

---

## Task 4: Update BlockChip rendering in Slint

**Files:**
- Modify: `crates/adapter-gui/ui/pages/project_chains.slint` — BlockChip component

- [ ] **Step 1: Update BlockChip to show thumbnail**

Replace the icon Image + accent-color rectangle with:

```slint
// Thumbnail (when available)
if root.block.has-thumbnail : Image {
    x: 2px;
    y: 2px;
    width: parent.width - 4px;
    height: parent.height - 4px;
    source: root.block.thumbnail;
    image-fit: contain;
    opacity: root.block.enabled ? 1.0 : 0.4;
}

// Fallback icon (when no thumbnail)
if !root.block.has-thumbnail : Image {
    // ... keep existing icon rendering
}
```

Keep:
- The accent-color border (2px, color by type)
- The LED status dot (bottom-right corner)
- The type label text (top-left, only when no thumbnail)
- Selected state highlight
- Drag overlay

- [ ] **Step 2: Adjust dimensions if needed**

Thumbnails have varying aspect ratios (38x64 to 108x64). The BlockChip is 84x84.
Use `image-fit: contain` to preserve aspect ratio inside the square chip.
Optionally increase chip height to ~100px for better proportions.

- [ ] **Step 3: Test visually**

```bash
cargo run -p adapter-gui
```

Verify:
- Blocks with thumbnails show the pedal image
- Blocks without thumbnails show the current icon fallback
- Enabled/disabled opacity works
- Drag-and-drop still works
- Selection highlight still works

- [ ] **Step 4: Commit**

```
git add crates/adapter-gui/ui/pages/project_chains.slint
git commit -m "Render block thumbnails in chain view BlockChip"
```

---

## Task 5: Cache decoded images (performance)

**Files:**
- Modify: `crates/adapter-gui/src/lib.rs`

- [ ] **Step 1: Add image cache**

Decoding PNGs on every UI rebuild is wasteful. Cache decoded `slint::Image` by `(effect_type, model_id)`:

```rust
use std::collections::HashMap;
use std::sync::OnceLock;

static THUMBNAIL_CACHE: OnceLock<HashMap<(String, String), slint::Image>> = OnceLock::new();

fn get_or_decode_thumbnail(effect_type: &str, model_id: &str) -> (slint::Image, bool) {
    let cache = THUMBNAIL_CACHE.get_or_init(HashMap::new);
    // ... lookup or decode and insert
}
```

Use `std::sync::Mutex<HashMap>` if OnceLock is too restrictive (lazy population).

- [ ] **Step 2: Verify performance**

Rebuild chain view multiple times — no visible lag.

- [ ] **Step 3: Commit**

```
git add crates/adapter-gui/src/lib.rs
git commit -m "Cache decoded thumbnail images for performance"
```

---

## Notes

### Thumbnail sizing
- Current BlockChip: 84x84px
- MOD thumbnails: ~38x64 to ~108x64px (portrait orientation)
- Consider making BlockChip slightly taller (84x100) for better pedal proportions
- Use `image-fit: contain` with transparent padding

### Generic thumbnails for native blocks
- 161 model_ids total, ~80 are body IRs (acoustic guitar)
- Body IRs can all share one `body/_default.png`
- Focus custom thumbnails on: preamp (6), amp (12), cab (12), gain (4), delay (6)
- Everything else uses `_default.png` per type

### Future: LV2 plugin thumbnails
When block-lv2 is implemented, each LV2 plugin will have a thumbnail from `plugins/{name}.lv2/modgui/thumbnail*.png`. The `block-thumbnails` crate can be extended to include those.
