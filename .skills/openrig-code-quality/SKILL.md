---
name: openrig-code-quality
description: "Use when writing, editing, or refactoring any code. Prevents coupling, duplication, bad design. Apply BEFORE writing code, not after. Covers: data ownership, zero coupling, single source of truth, naming, UI rules, impact analysis, safe refactoring."
---

# Code Quality — Architecture Checklist

Mandatory rules for writing code. Apply BEFORE writing, not after. No exceptions.

## STOP — Check Before You Code

### 1. Data Ownership

- [ ] Information defined in the RIGHT place? (owner module, not consumer)
- [ ] Reading from source, or duplicating/inferring?
- [ ] Using `starts_with()`, `contains()`, string matching to determine type/brand? → **WRONG**

### 1b. Separation of Concerns (Business vs Presentation)

- [ ] **NEVER mix UI/visual config in business logic code** — colors, fonts, panel sizes, brand strip colors are GUI concerns
- [ ] Business logic crates (block-preamp, block-gain, etc.) define ONLY: id, display_name, brand, backend_kind, schema, validate, build
- [ ] Visual config (panel_bg, panel_text, brand_strip_bg, model_font) lives in the GUI layer (adapter-gui)
- [ ] Visual config should be in configuration files (YAML/JSON) in the adapter-gui assets, NOT in Rust structs
- [ ] Adding or changing a color/font NEVER requires recompiling a business logic crate

### 2. Zero Coupling

- [ ] Code references specific model IDs, brand names, effect types? → **WRONG**
- [ ] Adding a new model requires changing this file? → **WRONG**
- [ ] Match/if chain grows when new models are added? → **WRONG**
- [ ] Consumer knows about specific producers? → **WRONG**

### 3. Single Source of Truth

- [ ] `DISPLAY_NAME` ONLY in module const (never in schema, never hardcoded elsewhere)
- [ ] `brand` ONLY in MODEL_DEFINITION (never inferred from model_id)
- [ ] Colors ONLY in model visual config (never hardcoded in UI)
- [ ] String appears in 2+ places? → extract to const
- [ ] **ZERO string literals in comparisons** — `==`, `match`, `if` must use `const`. Never `"preamp"`, always `EFFECT_TYPE_PREAMP`
- [ ] Effect types, brands, model IDs — ALL constants, never inline strings

### 4. Naming & Files

- [ ] Module files prefixed: `native_`, `nam_`, `ir_`
- [ ] `DISPLAY_NAME` does NOT contain brand name
- [ ] Commits in English, no `Co-Authored-By`

### 5. UI / Slint

- [ ] No hardcoded colors by model_id or brand in Slint
- [ ] No knob SVGs (knobs are Slint components only)
- [ ] Brand logos: `if brand == CONST` for `@image-url` (Slint limitation, acceptable)
- [ ] Panel editor works generically — no type-specific logic

### 6. No Trash

- [ ] No serde aliases for old names
- [ ] No dead/commented code
- [ ] No workarounds/hacks
- [ ] Renamed something? → ALL references updated (code + YAML data files + presets)

### 7. Impact Analysis (NEW — from real failures)

Before making a change, verify:
- [ ] **Build system**: Does `build.rs` depend on file names? (e.g., `starts_with("compressor_")` breaks if file renamed to `native_compressor_`)
- [ ] **UI capabilities**: Does the target UI component support ALL widget types needed? (file picker, bool toggle, numeric, enum)
- [ ] **Callback chain**: Are ALL callbacks connected through the full chain? (model → crate → catalog → adapter-gui → Slint)
- [ ] **Window sizing**: If changing window content, does the window size accommodate it?

### 8. Safe Refactoring

- [ ] **Never use `sed -i` on Slint files** — encoding issues can empty the file. Use Edit tool instead
- [ ] **After renaming files**: clean + rebuild (`cargo clean -p <crate> && cargo build`)
- [ ] **After changing struct fields**: update ALL modules that construct the struct
- [ ] **Test visually** before committing UI changes — don't assume it looks right
- [ ] **One concern per commit** — don't mix refactoring with feature changes

## Anti-Patterns

```
❌ if model_id.starts_with("marshall") { "marshall" }
   // WRONG: inferring from string

❌ match model_id { "american_clean" => color(...) }
   // WRONG: hardcoding by model_id

❌ pub const DISPLAY_NAME: &str = "Marshall JCM 800";
   // WRONG: brand in display name

❌ if effect_type == "preamp" { ... }
   // WRONG: string literal in comparison

❌ #[serde(alias = "amp_head")]
   // WRONG: legacy alias

❌ use_panel_editor: true  // for ALL types without checking UI supports them
   // WRONG: enabling feature without verifying capability

❌ // In block-gain/src/nam_ibanez_ts9.rs:
   pub const MODEL_DEFINITION = GainModelDefinition {
       panel_bg: [0x1a, 0x5c, 0x2a],  // UI color in business logic!
       model_font: "Permanent Marker", // UI font in business logic!
   };
   // WRONG: visual config in business logic crate. Move to adapter-gui config

❌ sed -i '' 's/old/new/g' file.slint
   // WRONG: sed can empty Slint files. Use Edit tool
```

## Correct Patterns

```
✅ let brand = catalog_entry.brand;          // Data from source
✅ let color = catalog_entry.panel_bg;       // Data from source

✅ pub const MODEL_DEFINITION = PreampModelDefinition {
       display_name: DISPLAY_NAME,           // Const, no brand
       brand: "marshall",                     // Defined here, read elsewhere
       panel_bg: [0xb8, 0x98, 0x40],        // Visual config in model
   };

✅ private property <color> panel-bg:
       root.block-model-options[index].panel_bg;  // UI reads from data

✅ // Before renaming files, check build.rs
   grep "starts_with\|stem ==" crates/block-*/build.rs
```

## YAML Data Files

When renaming effect types, models, or identifiers:
- Update `project.yaml` in project root
- Update `preset.yaml` if exists
- Update ANY yaml files the user mentions
- **Never** add serde aliases — update the data instead
- Search: `grep -rn "old_name" **/*.yaml`

## Review Trigger

After writing code:
1. Add new model WITHOUT touching adapter-gui? If no → coupling
2. Change brand color WITHOUT touching Slint? If no → coupling
3. File has match/if listing specific models? → coupling
4. `cargo build` passes? → required before commit
5. Visual result matches expectation? → test before commit

## Red Flags — STOP and Redesign

- Adding a model requires changes in 3+ files
- Match arm count equals number of models
- Consumer imports producer's internal types
- Same string appears in code AND Slint AND YAML
- Feature flag enables something the UI can't handle
- "Quick fix" that hardcodes a value
