---
name: openrig-code-quality
description: Use when writing, editing, or refactoring code in this project — language-agnostic methodology rules (zero coupling, single source of truth, separation of concerns, file organization, naming, anti-patterns)
---

# Code Quality — Architecture Methodology

Methodology rules for ANY code in this project. Apply BEFORE writing, not after. No exceptions.

This skill is **language-agnostic**: it covers methodology principles (decoupling, ownership, naming, file organization) that apply identically to Rust, Slint, Python, shell, YAML, etc. Language-specific rules (Cargo workflow, Slint-only gotchas) live in:

- `rust-best-practices` — Rust idioms + OpenRig Cargo workflow (validate.sh, cargo clean, zero warnings, cfg guards)
- `slint-best-practices` — Slint UI rules (file size cap, sed-safety, `@image-url` constraints)

Premissas gerais do projeto (Superpowers obrigatórios por situação, rastreabilidade de issue, distribuição cross-platform, alterações no SO da placa, atualização de documentação) vivem em `CLAUDE.md` e são carregadas em toda conversa. Esta skill cobre apenas as regras de **metodologia** de qualidade.

---

## STOP — Check Before You Code

### 1. Data Ownership

- [ ] Information defined in the RIGHT place? (owner module, not consumer)
- [ ] Reading from source, or duplicating/inferring?
- [ ] Using `starts_with()`, `contains()`, string matching to determine type/brand? → **WRONG**

### 1b. Separation of Concerns (Business vs Presentation)

- [ ] **NEVER mix UI/visual config in business logic code** — colors, fonts, panel sizes, brand strip colors are GUI concerns
- [ ] Business logic modules define ONLY: id, display_name, brand, backend_kind, schema, validate, build
- [ ] Visual config (panel_bg, panel_text, brand_strip_bg, model_font) lives in the GUI layer
- [ ] Visual config should be in configuration files (YAML/JSON) in the GUI assets, NOT in business logic structs
- [ ] Adding or changing a color/font NEVER requires recompiling a business logic module

### 2. Zero Coupling

- [ ] Code references specific model IDs, brand names, effect types? → **WRONG**
- [ ] Adding a new model requires changing this file? → **WRONG**
- [ ] Match/if chain grows when new models are added? → **WRONG**
- [ ] Consumer knows about specific producers? → **WRONG**

### 3. Single Source of Truth

- [ ] `DISPLAY_NAME` ONLY in the owning module (never in schema, never hardcoded elsewhere)
- [ ] `brand` ONLY in the model definition (never inferred from model_id)
- [ ] Colors ONLY in the model visual config (never hardcoded in UI)
- [ ] String appears in 2+ places? → extract to const
- [ ] **ZERO string literals in comparisons** — `==`, `match`, `if` must use `const`. Never `"preamp"`, always `EFFECT_TYPE_PREAMP`
- [ ] Effect types, brands, model IDs — ALL constants, never inline strings

### 4. Naming

- [ ] Module files prefixed by backend (e.g. `native_`, `nam_`, `ir_`, `lv2_`)
- [ ] `DISPLAY_NAME` does NOT contain brand name (brand is its own field)
- [ ] Commits in English, no `Co-Authored-By` trailers
- [ ] Branch names follow `feature/issue-N` or `bugfix/issue-N` (no description suffix)

### 5. No Trash

- [ ] No serde aliases for old names — update the data instead
- [ ] No dead/commented code
- [ ] No workarounds/hacks
- [ ] Renamed something? → ALL references updated (code + YAML data files + presets)

### 6. Impact Analysis (from real failures)

Before making a change, verify:
- [ ] **Build system**: Does any build script depend on file names? (e.g., `starts_with("compressor_")` breaks if file renamed to `native_compressor_`)
- [ ] **UI capabilities**: Does the target UI component support ALL widget types needed? (file picker, bool toggle, numeric, enum)
- [ ] **Callback chain**: Are ALL callbacks connected through the full chain? (model → crate → catalog → adapter-gui → Slint)
- [ ] **Window sizing**: If changing UI content, does the window size accommodate it?

### 7. Safe Refactoring

- [ ] **After changing struct fields**: update ALL modules that construct the struct
- [ ] **Test visually** before committing UI changes — don't assume it looks right
- [ ] **One concern per commit** — don't mix refactoring with feature changes

### 8. Responsive UI

- [ ] **All UI elements must be responsive** — never invade adjacent areas
- [ ] Elements must adapt to window/panel size
- [ ] No hardcoded absolute positions that break at different sizes
- [ ] Test with minimum and maximum window sizes before committing
- [ ] Overflow/clip must be handled — if content doesn't fit, it should scroll or truncate, never overflow

### 9. File Organization — ONE RESPONSIBILITY PER FILE (ABSOLUTE)

- [ ] **One concern per file — no exceptions.** If you can describe a file with "and", it has too many responsibilities
- [ ] **`lib.rs` (or equivalent module entry) is for re-exports only** — NEVER implement logic there; move it to a named module
- [ ] Configuration files organized by component/domain (e.g., `visual_config/preamp.rs`, `visual_config/delay.rs`)
- [ ] A file with a match/if that grows with every new model → **WRONG, split by component**
- [ ] If a file has 50+ lines of match arms → it needs to be split immediately
- [ ] **God files are forbidden** — a file that 10+ different features touch is a god file; split it
- [ ] New feature? New file. Don't add to an existing file that already has a different concern

> Concrete file size limits per language live in `rust-best-practices` (600 lines for `.rs`) and `slint-best-practices` (500 lines for `.slint`).

**Known god files — never expand further (tracked in issue #276). Check current size before touching:**
- `crates/adapter-gui/src/lib.rs` — split in progress
- `crates/project/src/block.rs` — split in progress
- `crates/block-core/src/lib.rs` — split in progress
- `crates/block-core/src/param.rs` — split in progress

**Anti-patterns:**
```
❌ Adding a new function to adapter-gui/src/lib.rs
   // WRONG: already a god file. Create a new module instead.

❌ lib.rs with 200 lines of business logic
   // WRONG: lib.rs = re-exports only

❌ A match arm in block.rs growing from 13 to 14 branches
   // WRONG: the dispatch belongs in each block's own crate via trait

✅ crates/adapter-gui/src/device.rs — only device management
✅ crates/adapter-gui/src/project.rs — only project persistence
✅ crates/adapter-gui/src/chain.rs — only chain editing
```

### 10. Test Coverage (OBRIGATORIO)

- [ ] **Toda feature/bugfix DEVE ter testes** — sem exceção
- [ ] Nomenclatura: `<behavior>_<scenario>_<expected>` (ex: `validate_project_rejects_empty_chains`)
- [ ] Testar comportamento real, não mocks de fachada
- [ ] **Builds que dependem de assets externos**: marcar como ignored / opt-in
- [ ] **Registry tests**: iterar sobre TODOS os modelos via registry (schema, validate, build)
- [ ] Helpers de teste no próprio módulo — sem crate de test-utils separado

> Detalhes Rust-específicos (golden samples `1e-4`, `#[cfg(test)] mod tests`, `#[ignore]`, `cargo test --workspace`) ficam em `rust-best-practices`.

**Anti-Pattern (Testes):**
```
❌ Commitar código sem testes
   // WRONG: código sem teste é dívida técnica

❌ Testar build() de modelo IR/NAM/LV2 sem marcar ignored
   // WRONG: depende de assets externos, falha no CI

❌ Criar crate test-utils separado
   // WRONG: cada módulo deve ser autossuficiente em testes

❌ Usar mockall ou frameworks de mock
   // WRONG: testar código real, não mocks
```

---

## YAML Data Files

When renaming effect types, models, or identifiers:
- Update `project.yaml` in project root
- Update `preset.yaml` if exists
- Update ANY yaml files the user mentions
- **Never** add serde aliases — update the data instead
- Search: `grep -rn "old_name" **/*.yaml`

---

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

❌ // UI color/font in a business-logic module:
   pub const MODEL_DEFINITION = GainModelDefinition {
       panel_bg: [0x1a, 0x5c, 0x2a],   // UI color in business logic!
       model_font: "Permanent Marker", // UI font in business logic!
   };
   // WRONG: visual config in business logic crate. Move to UI config
```

## Correct Patterns

```
✅ // Business data from catalog
   let brand = catalog_entry.brand;
   let type_label = catalog_entry.type_label;

✅ // Visual config from UI layer (NOT from business crate)
   let vc = visual_config::for_model(&item.brand, &item.model_id);
   let panel_bg = vc.panel_bg;

✅ // Model definition has ONLY business logic
   pub const MODEL_DEFINITION = PreampModelDefinition {
       id: MODEL_ID,
       display_name: DISPLAY_NAME,   // No brand in name
       brand: "marshall",            // Business data
       backend_kind: PreampBackendKind::Nam,
       schema, validate, build,      // Business logic only
       // NO colors, fonts, or visual config here
   };

✅ // Before renaming files, check build.rs
   grep "starts_with\|stem ==" crates/block-*/build.rs
```

---

## Review Trigger

After writing code:
1. Add new model WITHOUT touching the UI layer? If yes → coupling
2. Change brand color WITHOUT touching the UI? If yes → coupling
3. File has match/if listing specific models? → coupling
4. Visual result matches expectation? → test before commit

## Red Flags — STOP and Redesign

- Adding a model requires changes in 3+ files
- Match arm count equals number of models
- Consumer imports producer's internal types
- Same string appears in code AND UI AND YAML
- Feature flag enables something the UI can't handle
- "Quick fix" that hardcodes a value
- Path is hardcoded as string literal

---

## Living Document

This skill is a LIVING DOCUMENT. Every time the user corrects a methodology mistake:
1. Identify the violated principle
2. Add a rule or anti-pattern to this skill (if methodology) or to `rust-best-practices` / `slint-best-practices` (if language-specific)
3. Commit the updated skill

This ensures the same mistake is never repeated.
