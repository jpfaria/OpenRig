---
name: openrig-code-quality
description: "Use when writing, editing, or refactoring any code. Prevents coupling, duplication, bad design. Apply BEFORE writing code, not after. Covers: data ownership, zero coupling, single source of truth, naming, UI rules, impact analysis, safe refactoring."
---

# Code Quality — Architecture Checklist

Mandatory rules for writing code. Apply BEFORE writing, not after. No exceptions.

## MANDATORY — Load These Skills First

Before doing ANYTHING, invoke the relevant superpowers skills using the Skill tool:

| Situation | Skill to invoke |
|-----------|----------------|
| Adding a feature or new behavior | `superpowers:brainstorming` |
| Implementing a feature or bugfix | `superpowers:test-driven-development` |
| Debugging a bug or test failure | `superpowers:systematic-debugging` |
| Have a plan ready to execute | `superpowers:executing-plans` |
| Work is complete, about to claim done | `superpowers:verification-before-completion` |
| Received code review feedback | `superpowers:receiving-code-review` |
| About to finish a branch | `superpowers:finishing-a-development-branch` |

**These are not optional.** If you skip them, you are violating the project's development process.

---

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
✅ // Business data from catalog
   let brand = catalog_entry.brand;
   let type_label = catalog_entry.type_label;

✅ // Visual config from adapter-gui (NOT from business crate)
   let visual = visual_config_for_model(&brand, &model_id);
   let panel_bg = visual.panel_bg;

✅ // Model definition has ONLY business logic
   pub const MODEL_DEFINITION = PreampModelDefinition {
       id: MODEL_ID,
       display_name: DISPLAY_NAME,  // No brand in name
       brand: "marshall",           // Business data
       backend_kind: PreampBackendKind::Nam,
       schema, validate, build,     // Business logic only
       // NO colors, fonts, or visual config here
   };

✅ // UI reads visual config from adapter-gui layer
   private property <color> panel-bg:
       root.block-model-options[index].panel_bg;

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

### 9. Responsive UI

- [ ] **All UI elements must be responsive** — never invade adjacent areas
- [ ] Elements must adapt to window/panel size
- [ ] No hardcoded absolute positions that break at different sizes
- [ ] Test with minimum and maximum window sizes before committing
- [ ] Overflow/clip must be handled — if content doesn't fit, it should scroll or truncate, never overflow

### 10. File Organization — ONE RESPONSIBILITY PER FILE (ABSOLUTE)

- [ ] **One concern per file — no exceptions.** If you can describe a file with "and", it has too many responsibilities
- [ ] **500 lines max** — if a file exceeds 500 lines, it MUST be split before adding anything new
- [ ] **lib.rs is for re-exports only** — NEVER implement logic in lib.rs; move it to a named module
- [ ] Configuration files organized by component/domain (e.g., `visual_config/preamp.rs`, `visual_config/delay.rs`)
- [ ] A file with a match/if that grows with every new model → **WRONG, split by component**
- [ ] If a file has 50+ lines of match arms → it needs to be split immediately
- [ ] **God files are forbidden** — a file that 10+ different features touch is a god file; split it
- [ ] New feature? New file. Don't add to an existing file that already has a different concern

**Known god files to never expand further (tracked in issue #276):**
- `crates/adapter-gui/src/lib.rs` (9441 lines) — split in progress
- `crates/project/src/block.rs` (1498 lines) — split in progress
- `crates/block-core/src/lib.rs` (978 lines) — split in progress
- `crates/block-core/src/param.rs` (1207 lines) — split in progress

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

### 11. Documentation Always Up-to-Date

- [ ] **CLAUDE.md must always reflect current state** — when creating, removing, or changing models, block types, parameters, features, or screens, update the corresponding section in CLAUDE.md
- [ ] Added a new model? → update the "Tipos de bloco" table and parameter list
- [ ] Added a new block type? → add it to the table with description and models
- [ ] Changed parameters? → update "Parâmetros comuns" section
- [ ] Added a new screen/feature? → update "Telas principais" section
- [ ] Removed something? → remove from CLAUDE.md too, no stale documentation
- [ ] Documentation is part of the task — not a separate step. If you change code, you change docs in the same commit

### 12. Mandatory Documentation on Every Feature/Change

- [ ] **Every PR must include doc updates** — CLAUDE.md, AGENTS.md, issue description
- [ ] **New block types** → add to "Tipos de bloco" table in CLAUDE.md, update YAML examples
- [ ] **New data model structs** → document in CLAUDE.md architecture section and AGENTS.md
- [ ] **New audio processing behavior** → document in CLAUDE.md "Configuração de áudio" section
- [ ] **Issue closed** → update issue with final status, what was done, what's pending
- [ ] **Specs and plans** → keep in `docs/superpowers/specs/` and `docs/superpowers/plans/`, update when design changes
- [ ] **NEVER close a PR without updating docs** — undocumented features are technical debt

### Anti-Pattern
```
❌ Merge PR with 54 commits and no doc updates
   // WRONG: features become invisible, next developer has no context

✅ Every PR updates CLAUDE.md, AGENTS.md, and closes the issue with summary
   // RIGHT: docs always reflect current state
```

### 13. Test Coverage (OBRIGATORIO)

- [ ] **Toda feature/bugfix DEVE ter testes** — sem exceção
- [ ] Testes dentro do módulo: `#[cfg(test)] mod tests { ... }`
- [ ] Nomenclatura: `<behavior>_<scenario>_<expected>` (ex: `validate_project_rejects_empty_chains`)
- [ ] Sem frameworks externos — usar `assert!`, `assert_eq!`, `assert!(result.is_err())`
- [ ] **DSP nativo**: testar com golden samples, tolerância `1e-4`
- [ ] **NAM/LV2/IR builds**: marcar `#[ignore]` (dependem de assets externos)
- [ ] **Registry tests** para block-* crates: iterar sobre TODOS os modelos via registry (schema, validate, build)
- [ ] Helpers de teste no próprio módulo — sem crate de test-utils separado
- [ ] `cargo test --workspace` DEVE passar antes de qualquer commit
- [ ] Cobertura local: `scripts/coverage.sh` (requer `cargo-llvm-cov`)

### Anti-Pattern (Testes)
```
❌ Commitar código sem testes
   // WRONG: código sem teste é dívida técnica

❌ Testar build() de modelo IR/NAM/LV2 sem #[ignore]
   // WRONG: depende de assets externos, falha no CI

❌ Criar crate test-utils separado
   // WRONG: cada crate deve ser autossuficiente em testes

❌ Usar mockall ou frameworks de mock
   // WRONG: testar código real, não mocks
```

## Living Document

This skill is a LIVING DOCUMENT. Every time the user corrects a software engineering mistake:
1. Identify the violated principle
2. Add a rule or anti-pattern to this skill
3. Commit the updated skill

This ensures the same mistake is never repeated.
