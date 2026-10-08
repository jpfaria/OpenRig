# File organization

God-files appear when feature-specific logic lands in shared files. The hard rule:

> **Code is shared ONLY when 2+ features use it.** Feature-specific logic lives in the feature's module.

## The law: one file, one responsibility

**A production file does ONE thing** — one responsibility is one reason to change. The test is verbal: describe the file in one sentence; if you need "and", it is two files.

**Only test files may be large.** Tests have no line cap. Production files (non-test `.rs`, `.slint`) have a cap AND the responsibility law — and the law is what rules: 300 lines doing 4 things already breaks it, even under the cap.

A new responsibility never goes at the end of an existing file — it is born in its own file. Before adding ANY line to a production file, look at its size and its responsibility.

A split always preserves behaviour: it moves code, changes nothing, and no existing test is rewritten to fit the new shape.

### The header declaration

Every production file — `.rs`, `.slint`, `build.rs`, examples — opens with a line declaring the ONE thing it does:

```rust
//! Responsibility: rebuilds a chain runtime in place
```
```slint
// Responsibility: renders one block tile inside a chain row
```

`validate.sh` (check 1) fails two things:

| Situation | Result |
|---|---|
| Production file without the line | ❌ FAIL |
| Declaration with a conjunction or a list (`and`, `+`, `,`, `/`) | ❌ FAIL — the file is confessing it does two things |

Test files are exempt: their name already says what they cover. Every production file in the repository declares, so the whole-repo mode (`validate.sh crates`) fails, it does not warn.

Writing the sentence IS the analysis. When it will not come out without an "and", the file has two owners and new code goes to a new file.

## Where each thing lives

| Situation | Where it lives |
|---|---|
| Constant, type or fn used by 2+ crates or 2+ features | a shared crate (`block-core`, `domain`, `project`) |
| Logic of ONE model (its schema, its DSP) | the block crate that owns it |
| A model's visual config (colours, font) | the block crate's `model_visual.rs` — NEVER in `MODEL_DEFINITION` |
| Wiring of ONE Slint widget | its own `*_wiring.rs` file |
| Audio-thread hot path | the `engine` crate, split by responsibility |

## Anti-patterns

```
❌ a new match/if in a central crate for every new model
❌ adapter-gui/src/lib.rs holding thousands of lines of Slint callbacks
❌ match branches that grow per effect_type in a shared file
❌ visual config inside MODEL_DEFINITION (mixes business and GUI)
❌ a model_id string literal in a shared file
```

## Correct patterns

```
✅ each block-* crate exports <crate>_model_visual(id) — the UI reads the look without touching business logic
✅ adapter-gui split into one *_wiring.rs per feature
✅ engine runtime split by responsibility
✅ a Slint ternary per model_id in ONE component (block_panel_brand_strip.slint) — the authorised exception
```

## Size caps (validate.sh)

- `.rs` (non-test): **600 LOC**
- `.slint`: **500 LOC**
- test `.rs`: **no cap** — the only size exception in the repo; `validate.sh` does not measure them
- `lib.rs` / `mod.rs`: re-exports only, < 100 LOC

### The debt ratchet

`validate.sh` keeps `DEBT_FILES`, the baseline LOC of each production file that was already over the cap. The list is a ratchet, not an amnesty:

| Situation | Gate result |
|---|---|
| NEW file over the cap | ❌ FAIL — split before committing |
| Debt file that **grew** past its baseline | ❌ FAIL — it may never grow |
| Debt file that shrank, still over the cap | ⚠️ WARN + lower its baseline in the same commit |
| Debt file that dropped **under** the cap | ❌ FAIL — delete its line (the debt is paid) |

A file is never added to the list; it only shrinks. **The list is empty today**, and that is the normal state: a file born large is the "new file over the cap" FAIL, not a debt entry.

### Edit-time guard

The dev-rules plugin's `line-cap-guard` (PreToolUse) **denies an Edit/Write that grows** a file already over the cap; an edit that shrinks passes, so a split is never blocked by itself. Its caps come from the repo's `.dev-rules.json` (`line_caps`) — the same numbers as `validate.sh`.

### Splitting a file behind `cfg`

Half of `infra-cpal` only compiles on Linux + JACK, and the development machine is macOS: a green `cargo build` there **says nothing** about the other path. Splitting gated files breaks in two places:

1. **The new file's imports.** The item stays behind the right `cfg`, but the `use` that reaches it does not — it points at the old module, or asks for a symbol that does not exist under that `cfg` (`select_host_for_enumeration` exists only outside JACK; `jack_server_is_running` only inside).
2. **The module declaration in `lib.rs`.** Inserting `mod new_module;` right after a `#[cfg(...)]` **steals the attribute from the next module**, which then compiles ungated on the other platform.

Before committing a split that touches gated code:

- check the declaration in `lib.rs` — each new `mod` carries the SAME `cfg` as the file it came from, and the neighbour's `cfg` stays on the neighbour;
- for each new file, list its free identifiers and confirm the `use` that brings them exists under the SAME `cfg` they are used in;
- a re-export of the old path carries the item's `cfg`, not the file's.

CI confirms it (the `Test Suite` job of `test.yml`, on Ubuntu). There is no local substitute: `jack-sys` needs a Linux sysroot and does not cross-compile from macOS.

## Minimum font size in the UI

No app text may be smaller than **18px** — the size of the preset select itself, the legibility floor. The value lives once in `Theme.min-font` (`crates/adapter-gui/ui/theme.slint`); use the token instead of repeating the number. **The only exception, decided by the owner:** the text around a knob — its caption (`Theme.knob-caption-font`, 12px) and a selector's non-numeric positions (`Theme.knob-option-font`, 10px), which live in the knob's tight cell. Check 6 of `validate.sh` fails any `font-size` literal below the floor in our `.slint` files (the vendored `ui/modules/**` is out of scope: it only supplies icons), and `scripts/tests/min_font_size_test.sh` covers the check.

## LV2 plugins — `audio_mode` vs builder

The builder and the `audio_mode` must match. Mixing them up means a SIGSEGV or wasted CPU.

| The plugin is… | Builder | `ModelAudioMode` |
|---|---|---|
| 1 in / 1 out | `lv2::build_lv2_processor_full` with `[in], [out]` | `DualMono` or `MonoOnly` |
| 1 in / 2 out | `lv2::build_stereo_lv2_processor_full` with `[in], [L, R]` (the input gets the L/R mid) | `MonoToStereo` |
| 2 in / 2 out | `lv2::build_stereo_lv2_processor_full` | `TrueStereo` |
| 2 in / 1 out (sidechain) | `lv2::build_lv2_processor_full` | `DualMono` |
| 3 in / 2 out (stereo + sidechain) | `lv2::build_stereo_lv2_processor_full` with `[L, R, sidechain]` (the sidechain gets the L/R mid) | `TrueStereo` |

Classic symptom: a 4-port plugin declared `DualMono` → 2 dangling ports → SIGSEGV on the first write.

LV2 packages do not declare the mode by hand: `crates/project/src/block/disk_audio_mode.rs` reads the audio ports from the TTL and applies this table.
