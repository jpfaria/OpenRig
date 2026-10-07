---
name: openrig-code-quality
description: Use when writing, editing, or refactoring code in OpenRig — project-specific rules that COMPLEMENT (do not duplicate) xgodev `dev-rules` and `quality-gate` skills
---

# Code Quality — OpenRig-specific complement

This skill carries **only what `dev-rules` and `quality-gate` do not cover**:
the OpenRig-specific architecture, gitflow, i18n, audio invariants, file
inventory, and tooling. Anything generic (TDD/RED-first, docs synced same
commit, ownership/coupling/SoT, naming, file organization, DDD, verify
before done, no silent fallback, no skipped tests, communication, generic
red flags, living-document discipline) lives in **`dev-rules`** and is
the source of truth for those rules. Gate mechanics (dispatcher, JSON
parsing, bypass governance) live in **`quality-gate`**. Do not restate
either here; cite them.

---

## LAW ZERO — never assume when something is unclear; ask simply until it is clear

Before touching code, architecture or tests: if the scope, the data model, the expected behaviour, the right layer or ANY detail is not 100% clear, **STOP and ask** — one short, direct question at a time, until no doubt is left. Forbidden: "I'll go from memory and fix it later"; inventing a path; picking between A and B on your own when the owner did not pick.

**Why:** assuming has inverted what the owner asked and burned days — I/O inside the chain vs. outside it (system/binding); "hide the block" when the ask was to NOT have a block; test-after when the ask was test-first. Every wrong assumption cascades into rework and breaks trust.

**How to apply:** doubt → a simple question → wait for the answer → only then act. Do not bundle several assumptions in one go. Do not follow "it looks like the logical next step". This does NOT conflict with "don't ask the obvious" (agreed scope or a trivial default goes straight on): the rule applies when there is real ambiguity — between asking and assuming, **ask**.

---

## LAW — never assume or invent a UI layout; render it and check

It is **FORBIDDEN** to write, change or judge ("looks good") ANY layout/UI — `.slint`, positioning, spacing, hierarchy, alignment, visual components — **without first invoking `claude-plugin:ux-ui` (design/UX) + `slint-best-practices`** and **driving the work through them**: Slint's native layouts (`HorizontalLayout`/`VerticalLayout`/`GridLayout` with `spacing`/`padding` — never absolute `x`/`y` to align a cluster), hierarchy, empty/error states, targets ≥ 44px.

**The agent RENDERS and checks** — it does not guess:
- Use the project's headless renderer: `tools/slint-render` (slint-interpreter → PNG, outside the workspace). Build: `cargo build --release --manifest-path tools/slint-render/Cargo.toml`. Usage: `slint-render <file.slint> <Component> <out.png> [w] [h]`. For components embedded in the app, write a standalone `.slint` mockup with fake data (root `inherits Window`, explicit size) and render THAT.
- Open the PNG (Read) and check alignment/spacing/hierarchy **BEFORE saying "done"**. Guessing a layout and asking the owner to test it is a **forbidden anti-pattern** (it has burned a whole day of tokens shipping crooked screens).
- Touched or created a screen → invoke `claude-plugin:ux-ui` + `slint-best-practices` **BEFORE the first `.slint` line**, and build it in the language of `docs/gui/visual-language.md` (tokens, reusable components, mockup T01–T42); a screen that looks different from the mockup is not done.
- **Mandatory SELF-REVIEW before showing the PNG** — run the checklist and FIX it yourself; never hand over a raw render for the owner to catch the basics (he is not a designer):
  - Clear hierarchy (size/weight, not everything the same).
  - Semantic colour: distinct states/categories get colour + meaning (e.g. mono/stereo badges in different colours), not everything grey.
  - ALL CRUD actions on each item (create, **edit**, delete) — not just delete.
  - States: empty ("nothing yet" + an action), disabled, error, selected.
  - Domain rules reflected in the UI (e.g. mono = 1 channel → the 2nd channel disabled).
  - Grid alignment, consistent 8px spacing, usable touch targets, readable badges/labels.
- Only AFTER the self-review, the final visual pass in a **short loop with the owner** (he points at what is still crooked) — but the basics must already be solved.

---

## LAW — every push delivers an explicit handoff

After a `git push` on an agent branch whose result the owner must validate, the chat reply — same turn, unasked — carries the **validation checklist**: the main-folder command, the absolute `run:` line printed by `scripts/solver-setup.sh`, then numbered `1. [ ]` items of what only he can validate (golden path, the edge case behind the issue, adjacent flows that could regress, the expected result one per line). The same block goes on the issue. The exact format, with examples, is `docs/development/gitflow.md` → "Validation checklist".

**Why:** the owner has N agents opening branches at once. "Check the branch" is not an instruction — he needs the exact command and the list of what to open/click/type to see the change. Without it he either forgets to test or tests superficially.

**Anti-pattern:**
```
❌ "Pushed feature/issue-N. Shall I continue?"
   // WRONG: no command, no checklist.

❌ "Pushed 68ea1bcf. Changed chain_preset_wiring.rs."
   // WRONG: describes a file, not a validation.

❌ cd .solvers/issue-N && cargo run -p adapter-gui -- --mcp
   // WRONG: relative path and no OPENRIG_PLUGINS_ROOT — the app opens with no plugins. Paste the `run:` line of solver-setup.sh.
```

It applies to EVERY push the owner must validate, incremental ones included. The repetition is the point — he does not memorize branches, he reads the block and follows it.

---

## LAW — closing an issue requires a milestone

**BEFORE `gh issue close N`:** run `gh issue view N --json milestone` and confirm it has one. If not:

1. The milestone is the **active release version** — the plain semver `vX.Y.Z` being built (the highest `release/vX.Y.Z` with no tag). The `vX.Y.Z-dev.N` scheme is DEAD: **never create or reopen** a `-dev.N` milestone.
2. `gh issue edit N --milestone "vX.Y.Z"` → `gh issue close N`.

It applies equally to an issue created and closed in the same session. PRs too: `gh pr edit <N> --milestone "vX.Y.Z"` before the merge. A close as not-planned / duplicate / superseded takes NO milestone.

**Why it matters.** GitHub's release notes group by milestone. An issue or PR closed without one is missing from the changelog — a reader cannot tell it shipped in that release.

---

## LAW — docs sync: OpenRig's concrete layers

> The general rule ("docs in the SAME commit") is `dev-rules` LAW 2. Here are only OpenRig's **concrete layers** that must be touched:

| Layer | For | Update when |
|---|---|---|
| `docs/**/*.md` | humans (contributors, users) | audio behaviour, UI flow, block, parameter, screen, CLI, deploy or hardware changed |
| `CLAUDE.md` (root) | every Claude session | an invariant, the trade-off order or a general rule changed |
| `.claude/skills/*/SKILL.md` | future Claude sessions | OpenRig methodology, an anti-pattern, a gate, a process or a gitflow detail changed |
| `README.md` + `README.pt-BR.md` + `README.es-ES.md` | the world (3 languages) | tagline, feature list, install, build/deploy or a link changed |
| `site/` (3 languages, copy via `openrig-site-copy`) | the world | a user-facing feature shipped or changed |
| `CONTRIBUTING.md` | contributors | the contribution process changed |

Project knowledge lives in the repo, never in an agent's private memory.

**How to apply (OpenRig-specific):**
- Renamed a model/parameter/effect_type? → grep `docs/**`, `*.md`, `README*`, `CLAUDE.md` and every `.claude/skills/*/SKILL.md`.
- Changed the gate/build/deploy process? → update `openrig-code-quality`, `slint-best-practices` **and** the matching `docs/development/*.md`.
- Changed an invariant (latency, isolation, mixing)? → `CLAUDE.md` + `docs/architecture.md`.
- A README updated in one language without the other two is a regression.

---

## LAW — a new screen or string updates EVERY translation catalog

**Every user-visible string goes through i18n.** There are two catalogs, one per side:

- **Slint `@tr("key")`** → gettext. Add the key to `crates/adapter-gui/translations/adapter-gui.pot` and a translated `msgid "key"` + `msgstr "..."` to **EVERY** locale's `crates/adapter-gui/translations/<locale>/LC_MESSAGES/adapter-gui.po` (de_DE, en_US, es_ES, fr_FR, hi_IN, ja_JP, ko_KR, pt_BR, zh_CN — confirm with `ls translations/`). `build.rs` bundles the `.po` files.
- **Rust `t!("key")`** (`rust_i18n`) → `crates/adapter-gui/locales/<locale>.yml`, every locale.

Same commit as the component. **Never** leave an empty `msgstr ""` on a new key — the UI shows the **raw key** (`btn-load-preset` instead of "Load preset"): a new key without a catalog entry = a screen "with only the tags". There is no "translate later".

**Anti-pattern 1:** `text: "Raw text"` straight in a `.slint` (no `@tr()`). User-visible text is **NEVER** a literal. Visual symbols (`✓`, `▼`, …) become SVG via `@image-url`, not `Text`.

**Anti-pattern 2:** `@tr("new-key")` in a new component without touching any `.po`.

**Automated checks (`crates/adapter-gui/src/i18n_catalog_tests.rs`):**

1. `every_tr_key_has_translation_in_en_pt_es` — scans every `.slint` under `crates/adapter-gui/ui/`, extracts each `@tr("…")` (decoding `\u{NNNN}` and `\"`) and requires a non-empty `msgstr` in en_US, pt_BR and es_ES.
2. `no_raw_text_literals_in_settings_slint` — scans the Settings screen and fails when `text:` points at a non-`@tr()` literal. Widen its scope before adding `text: "x"` to any `.slint`.
3. `settings_screen_tr_keys_are_translated_in_pt_br` — a Settings-specific guard.

---

## LAW — an external service is an interface; the vendor is one implementation, chosen by config

Crash reporting, telemetry, cloud sync, any third-party SDK: the app talks to a **trait** it owns (e.g. `CrashReporter`), never to the vendor crate. The vendor (Sentry, …) is ONE implementation of that trait, and which implementation runs — or none — comes from **config** (`config.yaml`, system scope per ADR 0003), not from code. Everything the app hands the service (context, events) is vendor-neutral data (`serde_json::Value`, own structs). The vendor crate is imported in exactly one module: its implementation.

The service is best effort: a bad config, an unreachable endpoint or a failing implementation disables it with a warning in the session log — it never panics, blocks or slows the app, and never touches the audio thread.

**Why:** (#1070) Sentry was wired straight into the GUI (`sentry::init`, `before_send`, `sentry::protocol` types across modules), the provider fixed at compile time. The owner: "tinha que ser uma config.. deveria ter uma interface e o sentry uma implementacao". Swapping the vendor would have meant rewriting call sites, and a malformed DSN made `sentry::init` panic at startup.

**How to apply:** before the first line that imports a vendor SDK, write the trait + the config entry + a no-op implementation; the vendor goes behind it. Tests drive the trait (no-op / in-memory), plus one test of the vendor implementation over its in-memory transport.

---

## LAW — every new feature is a `Command` (GUI/MCP/gRPC parity)

**No state-changing operation lives in one frontend only.** The `Command` enum in `crates/application/src/command.rs` is the **single source of truth** of what the app can do. Every new feature that mutates `Project`/session:

1. **Is born as a `Command` variant** + (if observable) an `Event` variant, with a handler in the `LocalDispatcher`. Never as a `borrow_mut()` inside a frontend callback.
2. **GUI** dispatches via `dispatcher.dispatch(cmd)` and reacts to `Event`s — it never mutates `Project` directly.
3. **MCP** (`adapter-mcp`) gets the tool **automatically** (the schema is derived from `Command` via `application::command_schema`). No manual step — but the parity test (`tool count == command_variant_names().len()`) **must** stay green.
4. **gRPC** (`adapter-server`, when it exists): the same variant.

A feature that exists in a frontend but is **not** a `Command` is a **command-bus gap**, not a "frontend feature". Close it by adding the variant (e.g. `SetLanguage` — the language used to come from the env and could not be set). One `Command` per user operation.

**Why:** the core becomes gRPC + MCP + remote. If an operation exists only in the GUI, the agent (MCP) and the remote client (gRPC) are blind to it. Parity is not optional; it is the contract of the adapter architecture.

**How to apply:** before writing any state-changing flow — "is this a `Command`?" No? Create the variant first (TDD: a test in `local_dispatcher_tests.rs`), then wire the frontend.

**Anti-pattern:**
```
❌ on_language_changed => { session.project.borrow_mut().language = tag; }
   // WRONG: mutates in the callback. MCP/gRPC never see "change language".

❌ "this feature is GUI-only, it doesn't need a Command"
   // WRONG: every state operation is a Command. No frontend exception.
```
**Pattern:**
```
✅ Command::SetLanguage { tag } + Event::LanguageChanged + a LocalDispatcher handler
   → GUI dispatch(cmd); MCP tool automatic; gRPC the same variant.
```

---

## LAW — every read has GUI/MCP/gRPC/MIDI parity too

**The dual of `Command` is the `Query`.** If the GUI reads some state, every other transport must read it too. There is no "only the GUI needs this number". Meters, level peaks, latency probes, device lists, project YAML, scene/preset state, tuner readings, spectrum frames — every observation window the GUI has **must exist in `QueryKind`** (`crates/application/src/query_kind.rs`) and be served by **every adapter**: MCP as a resource or tool, gRPC as a method, MIDI as a SysEx/CC reply where it makes sense.

1. **Born as a `QueryKind` variant** + a handler in the GUI-thread drain (`serve_queries`) that serializes the state and answers. Never straight from the `RefCell<Project>` in an ad-hoc resource.
2. **MCP** gets `openrig://<name>` in `adapter-mcp`'s `resources.rs` (or a `read_*` tool when an action fits better), covered by the parity test.
3. **gRPC** gets the RPC with the same response shape.
4. **MIDI** (when the state matters to a footswitch): a CC/SysEx reply — optional for now, but the slot must exist in `QueryKind`.

A read the GUI has that is not in `QueryKind` = a **query-bus gap**. Close it in the same PR.

**Why:** the agent (MCP) and any remote client (gRPC) need to **see what the user sees** to decide well. Without read parity the agent is blind: it fixes a tone without seeing the clipping, adjusts a scene without seeing the current param. Command parity without Query parity is a hand without an eye.

**How to apply:** before adding any `meter_*`, `latency_*`, `peak_*`, `*_dbfs` property to the UI — "is this a `Query`?" No? Create the variant first (TDD), then wire the UI. The number the user sees on screen is in `QueryKind`, with no "it's just visual" exception.

**Anti-pattern:**
```
❌ row.meter_in_dbfs reads engine.pop_peak_dbfs directly in the GUI timer
   // WRONG: MCP/gRPC never see the meter. The agent is blind.

❌ "this is runtime state, not Project, so it doesn't need exposing"
   // WRONG: runtime != GUI-only. Every observation the user sees is part
   // of the contract with the other transports.
```
**Pattern:**
```
✅ QueryKind::ChainMeters → serve_queries serializes per chain id
   → the GUI reads through the same bridge (no parallel path)
   → MCP `openrig://meters` returns the serialized result
   → gRPC `GetChainMeters` returns the equivalent proto.
```

---

## Validation process (OpenRig gitflow — skip no step)

> "Verify before claiming done" is `dev-rules` LAW 3. This section is OpenRig's **concrete order** (`.solvers/`, conditional `cargo clean`, push before the gate):

1. **Implement** in `.solvers/issue-N/` (the isolated gitflow workspace).
2. **`cargo clean` when needed, BEFORE validating.** If the change involved a `build.rs`-generated file (registries), a rename/move, an added/removed `.rs`, a `Cargo.toml` dependency change, or any suspicion of a stale artifact in `target/` → `cargo clean` and rebuild before asking for validation. Otherwise the owner checks out the branch and his build breaks on a stale cache (e.g. `generated_registry.rs` pointing at a deleted module, `E0761` from an orphan `.rs`). When in doubt, clean.
3. **Do NOT run the full suite locally** (`cargo test --workspace`, `./scripts/patch-coverage.sh`): the PR's CI runs the tests and Codecov. Locally, cargo runs only in the two red-first rounds (all tests → one RED round → implement everything → one GREEN round, targeted tests only; `docs/testing.md`), then `cargo fmt --all -- --check` + the static `validate.sh` and ONE commit at the end. No cargo and no commit per step.
4. **`git push`** the branch (no PR yet).
5. **The owner validates on his machine** (validation checklist). Wait for explicit feedback before going on.
6. **Shared quality gate** — NEVER run locally. In Rust it compiles the workspace twice (base + branch); it runs in CI, in the `quality-gate` job of `.github/workflows/pr.yml`, when the PR opens (`docs/development/quality-gate.md`).
7. **Only THEN** the PR, and only when the owner asks — **always non-interactive**: push the branch first, then pass every field. **`--base` = the active `release/vX.Y.Z`** (feature/bug), or `main` (`release → main` and `hotfix`):

   ```bash
   gh pr create --repo jpfaria/OpenRig --base release/vX.Y.Z --head <branch> \
     --title "<type>(#<issue>): <summary>" --body "Closes #<issue>. …"
   ```

   NEVER run `gh pr create` without `--title`/`--body`/`--head` (or with the branch unpushed): the agent's shell has no TTY, gh opens its interactive prompt and **hangs** on a stdin that never comes, until the timeout (~8 min). Machine guard-rail: `gh config set prompt disabled` makes gh **fail at once** instead.

Do not invert:
- A PR before the owner's validation = rework when he finds a problem in real behaviour.
- A PR before the gate = CI fails and posts a sticky comment.

**Patch coverage fails the PR even when everything is green.** The target is the base's coverage, and a release PR accumulates the diff of every merge in the cycle. What drags it down is always the same: a new file with real logic and no test. Covering what is **pure** (projections, resolvers, parsers) pays fast; window wiring and stream construction are not coverable today and become an explicit decision (a seam or a written exclusion), never silent debt.

**This skill's focus (not the gate's):** audio invariants, OpenRig architecture decisions (Command/Query/i18n), the **semantic** quality of tests (behaviour ≠ coverage), brand/model anti-patterns. Mechanical metrics (fmt/lint/build/test/complexity/coverage) are the `quality-gate` skill.

---

## LAW — ONE FILE, ONE RESPONSIBILITY (the problem is NOT the line count)

**A production file does ONE thing.** Lines are a symptom; responsibility is the rule. A 300-line file doing 4 things breaks this law while passing the cap easily — and that is how every OpenRig god file started.

**Only test files may be large.** It is the repo's only size exemption.

**The sentence test (apply BEFORE the first line):** describe the file in one sentence starting with a verb. Needed "and"/"+"/a comma to list what it does → it is N files, not 1. That sentence is not a mental exercise: it goes **in the file header**, and the gate reads it.

```rust
//! Responsibility: rebuilds a chain runtime in place
```
```slint
// Responsibility: renders one block tile inside a chain row
```

`scripts/validate.sh` check 1 fails when a production file does not declare it, or when the declaration has a conjunction/list — the file confessing it does two things. Every production file in the repo declares, so the whole-repo run fails on any miss.

**About to add a new responsibility to an existing file? STOP.** It goes to a new file. `lib.rs`/`mod.rs` is a thin router (< 100 LOC, re-exports and delegation only).

**Why:** counting lines against a frozen allowlist let "just one more handler here" grow forever with a green gate. A line count never forbade the second responsibility; it only complained late.

### Rationalizations — every one means STOP and split

| Excuse | Reality |
|---|---|
| "It's just one more fn, the file is under the cap" | The cap is not the rule. A new responsibility = a new file, even at 50 lines. |
| "It's wiring, wiring belongs together" | ONE feature's wiring per file (`*_wiring.rs`). |
| "I'll split it later, in a refactor PR" | Later never comes. Split BEFORE it grows — cheap now, impossible later. |
| "They are related things" | Related ≠ the same responsibility. Rebuild and health check are related; they change for different reasons. |
| "The gate let it through" | Touched the file, you own its declaration. |
| "Declaring the header is bureaucracy" | It is the only way a machine can check intent. Can't write the sentence = you don't know what the file does. |

The debt ratchet (`DEBT_FILES`), the edit-time `line-cap-guard` and the cfg-split pitfalls are in `docs/development/file-organization.md`.

**CI — the gate MUST be able to fail:**
- `.github/workflows/test.yml` runs the workspace tests on PRs and pushes to `develop`, `release/**` and `main` **without `|| true`** — a red test fails the job. Coverage (llvm-cov/Codecov) is a report, not a gate.
- `scripts/validate.sh` runs over the **whole repo** in CI (`VALIDATE_STATIC_ONLY=1 ./scripts/validate.sh crates` — the static checks only: responsibility + size + inline tests + minimum font; fmt/clippy/slint belong to the shared gate).

---

## Test coverage — OpenRig specifics

> RED-first TDD = `dev-rules` LAW 1. "No skipped tests to go green" = `dev-rules` LAW 5. This section is the **concrete OpenRig plan**:

- Naming: `<behavior>_<scenario>_<expected>` (e.g. `validate_project_rejects_empty_chains`).
- **Builds that depend on external assets**: bundle a minimal fixture under `crates/<x>/tests/fixtures/` (see `crates/engine/tests/fixtures/plugins/`). The test ALWAYS runs.
- **A test that needs the owner's PRIVATE capture tree** (the real NAM/IR/LV2 library, too big to bundle): resolve the root via `OPENRIG_OWNER_PLUGINS` **or** by walking up from `CARGO_MANIFEST_DIR` to a sibling `OpenRig-plugins/plugins/source` (robust at any depth — main checkout or `.solvers/`), and **skip loudly** (`eprintln!` + `return`) when `None`. The owner's real rig is opt-in via `OPENRIG_OWNER_PROJECT`. **FORBIDDEN:** an absolute `/Users/<name>/…` path, and reading the live `~/.openrig/project.yaml` (its result changes when the owner edits the rig; hard-fails on any other machine). In CI (no tree) the test skips, never fails. A fixture for a small asset; env var + skip for the owner's large tree.
- **Registry tests**: iterate over ALL models through the registry (schema, validate, build).
- Test helpers live in the module itself — no separate test-utils crate. No `mockall` or mock frameworks — test real code.

### `#[ignore]` is FORBIDDEN (an OpenRig law that hardens LAW 5)

The workspace test run is the behaviour gate. A test marked `#[ignore]` does NOT take part — it becomes dead documentation. **Never** add `#[ignore]` (or its equivalents: `#[cfg(any())]`, `if false {}`, …). The target is **zero**.

"Reasonable" reasons that are NOT exceptions:

| Real case | The RIGHT way out |
|---|---|
| "depends on an external asset (NAM, IR, LV2)" | Bundle a minimal fixture under `tests/fixtures/`. ~1 MB is acceptable. |
| "needs --release for timing" | Make it a benchmark (`cargo bench`) or widen the tolerance in debug. Don't ignore. |
| "pending issue — the current behaviour is wrong" | The test asserts the CURRENT symptom or describes the regression; it breaks when the issue is fixed. Don't ignore. |
| "depends on an external FFI/dylib" | `build.rs` copies the dylib into `target/`; or skip per platform with `#[cfg(target_os = "...")]`. A cfg skip is OK; ignore is not. |
| "absolute paths of the dev machine" | Small asset: COPY it into the repo (`tests/fixtures/`). The owner's large tree: `OPENRIG_OWNER_PLUGINS`/walk-up + loud skip. NEVER `/Users/<name>/…` nor the live `~/.openrig/project.yaml`. |
| "too slow in CI" | Equivalent unit coverage + one sample path in the integration test. Don't ignore. |

Check: `grep -rn '#\[ignore' crates` must return NOTHING.

---

## YAML data files (OpenRig)

When renaming effect types, models, or identifiers:
- Update `project.yaml` in project root
- Update `preset.yaml` if exists
- Update ANY yaml files the user mentions
- **Never** add serde aliases — update the data instead (consistent with `dev-rules` "No Trash")
- Search: `grep -rn "old_name" **/*.yaml`

---

## Anti-patterns OpenRig (brand/model/effect_type)

> The "data ownership / single source of truth / zero coupling" principles are in `dev-rules`. Here are only the **concrete examples** in OpenRig's domain (brand, model_id, effect_type):

```
❌ if model_id.starts_with("marshall") { "marshall" }
   // WRONG: inferring from string

❌ match model_id { "american_clean" => color(...) }
   // WRONG: hardcoding by model_id

❌ pub const DISPLAY_NAME: &str = "Marshall JCM 800";
   // WRONG: brand in display name (brand is its own field)

❌ if effect_type == "preamp" { ... }
   // WRONG: string literal in comparison; use EFFECT_TYPE_PREAMP const

❌ #[serde(alias = "amp_head")]
   // WRONG: legacy alias

❌ use_panel_editor: true  // for ALL types without checking UI supports them
   // WRONG: enabling feature without verifying capability

❌ // UI color/font in a business-logic module:
   pub const MODEL_DEFINITION = GainModelDefinition {
       panel_bg: [0x1a, 0x5c, 0x2a],   // UI color in business logic!
       model_font: "Permanent Marker", // UI font in business logic!
   };
   // WRONG: visual config in MODEL_DEFINITION. Move it to the crate's model_visual.rs
```

**Correct patterns:**
```
✅ // Business data from catalog
   let brand = catalog_entry.brand;
   let type_label = catalog_entry.type_label;

✅ // Visual config from the block crate's model_visual.rs, never from MODEL_DEFINITION
   let vc = block_preamp::preamp_model_visual(&item.model_id);

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

### Naming OpenRig

- Module files prefixed by backend (e.g. `native_`, `nam_`, `ir_`, `lv2_`).
- `DISPLAY_NAME` does NOT contain the brand name (brand is its own field).
- Commits in English, no `Co-Authored-By` trailers.
- Branch names are `{type}/issue-N` — `feature`, `bug`, `docs`, `hotfix` — with no description suffix (`docs/development/gitflow.md`).

### Impact analysis OpenRig (from real failures)

- **Build system**: does any `build.rs` depend on a file name? (e.g. `starts_with("compressor_")` breaks when the file becomes `native_compressor_`).
- **UI capabilities**: does the block editor support EVERY widget type needed (file picker, bool toggle, numeric, enum)?
- **Callback chain**: is every callback connected along the whole chain (model → crate → catalog → adapter-gui → Slint)?
- **Window sizing**: if the UI content changed, does the window fit it?

---

## Responsive UI (OpenRig)

- Every element is responsive — it never spills into neighbouring areas.
- No hardcoded absolute positions that break at other sizes.
- Check the minimum AND the maximum window before committing.
- Overflow/clipping is handled — if it doesn't fit, scroll or truncate, never overflow.

---

## LAW — tests that contradict a pinned invariant: STOP, don't decide alone

If two tests demand incompatible behaviours and one of them is a **pinned** invariant (`volume_invariants_tests.rs`, any test pinning invariant 10 of `CLAUDE.md`):

- The pinned invariant **wins by default**. The other test is obsolete.
- **NEVER** weaken or edit the pinned invariant without the owner's explicit request (the only sanctioned way).
- **NEVER** poke at the audio path "to make both pass".
- Report to the owner in **1–2 sentences**: the conflict, which test is obsolete, and carry on with what does not depend on it.

---

## LAW — no business rules in the GUI. State → Command. SIMPLE.

The owner's criterion (do NOT reinterpret or recategorize):

- **Opening/closing a screen or window = a SCREEN rule.** It may stay in the GUI.
- **EVERY action that CHANGES STATE** (model, rig, project, config, persistence, runtime) **= a BUSINESS rule = necessarily a `Command`** dispatched to the dispatcher. The GUI only dispatches and renders — zero logic.
- `Command` is **pure domain**: it never imports a UI/Slint/view type. It may express intent through a domain key/enum.
- **FORBIDDEN** to audit this with a Python script (regex heuristics err and hide the work). The analysis is **item by item**, callback by callback, **recorded on the issue** as a checklist to work through.
- One file per responsibility: the dispatcher is a thin router; each handler in its own file.

The rule is the sentence above, literally: do not recategorize ("navigation is a screen", "language is a screen") or reopen the debate.

---

## LAW — a runtime/audio bug: INSTRUMENT before theorizing

An audio/real-time bug whose cause does not jump out of the code: **after the FIRST hypothesis fails, stop theorizing and instrument.** Add a measurement of the **real values** (sample rates, buffer sizes, elastic levels, underrun/xrun counters, which file/path) and run it yourself — the hardware battery (`OPENRIG_HW_TESTS=1`), the MCP resources (`openrig://routes`, `openrig://meters`) or your own app run. The numbers settle it in one round; chained hypotheses burn patience and credibility.

**How to instrument (RT-safe — invariant #8):**
- **NEVER** `eprintln!`/log on the audio thread (`process_input_f32`/`process_output_f32`/`pop`/`push`). I/O in the callback is forbidden.
- On the audio thread: bump a **`Relaxed` atomic** (underrun/xrun/load counter) and **drain/print off the thread** (GUI timer, loader, wiring). Model: `record_callback_load` / `xrun_count` / `underrun_count` in `crates/engine/src/runtime_load.rs`.
- In loaders/wiring (off the audio thread) a temporary tagged `eprintln!` (`[probe]`) is OK. OpenRig logs to **stderr** via `env_logger` (no file): run `cargo run … 2>/tmp/openrig.log` and `grep` the tag.
- `openrig://routes` gives what each output route pulled (callbacks, underruns, peak); the MCP does not expose every runtime value. For anything else: instrument or read stderr. **Never** state a runtime value you did not observe.
- Tag the probe, commit it as a separate `chore:`, and **revert** it once the cause is confirmed (or promote it to a permanent surfaced counter, like `xrun_count`/`underrun_count`).

**Why:** reasoning about the state instead of observing it produces fixes that don't fix; one log line with the real values nails the cause in the first round. A cost hypothesis read from the median misleads: low CPU does not crackle — an underrun/stall does, and only instrumentation shows it.

**How to apply (extra):** a resampling bug tied to a config (rate/buffer) → check whether the **already loaded** buffer is rebuilt on the change, not just new loads. An "intermittent stall" on a single light stream points at the **input↔output decoupling** (elastic buffer), not at DSP cost.

**Anti-pattern:**
```
❌ "it must be X" → fix → "it must be Y" → fix   (2+ hypotheses without observing)
❌ stating engine_sr/buffer/a count without reading the real value
❌ eprintln! inside process_input_f32 / pop  (I/O on the audio thread)
```
**Pattern:** one hypothesis failed → atomic counter + tagged off-thread dump → run at buffer 64 → observe underruns vs xruns → cause nailed → a test that reproduces it → fix.

**If the symptom is happening RIGHT NOW, capture before you think.** An intermittent bug gives one chance per occurrence. When the owner says it is happening, the very first action is a capture of the broken state while it lasts — a recording of the interface inputs, `openrig://routes` + `meters`, `sample <pid>` — in one parallel batch, before any analysis, issue or answer. Better: start a rolling recorder plus a 0.5 s MCP poller (one that survives MCP timeouts) as soon as an intermittent bug is first reported, so the window is already being recorded.

**A symptom on the owner's machine can come from another agent session.** Several solver sessions run test suites on the same machine all day, and a test that touches per-machine state (`config.yaml`, presets) with env-based isolation can leak across an async boundary and overwrite his real files — the symptom then looks like an app bug. Before blaming a `develop` regression: read the real config (fixture-looking ids such as `new_dev`, `dev_a`, `dev_b` are the smoking gun), compare its mtime with `ps aux | grep cargo` — including hung test *binaries* that swapped `$HOME` to a FIFO and never exited — hash the file, run the suspected tests, re-hash, and restore his file from the snapshot immediately with a backup kept aside. Config writes bind the path at DISPATCH time via `crates/application/src/app_config_persist.rs` (`persist_app_config` and its siblings, over the `*_at(path)` functions in `crates/infra-filesystem/src/app_config_io.rs`), and `with_tmp_home` flushes the worker before restoring `$HOME`: any NEW config write site goes through `app_config_persist`, never `persist_worker::run(|| save_app_config(...))`.

**A law inside the law — PROVE it with a test BEFORE announcing the finding.** Reading the code and saying "found it, it's X" is a HYPOTHESIS, not proof — and it hurts credibility when wrong. Before claiming the cause: write a DETERMINISTIC test that shows the defect (e.g. instead of "the LV2 worker runs inline", a test that schedules worker work and checks which thread runs it; it FAILING = proven). And "I proved defect X exists" ≠ "I proved X causes the OWNER's crackle" — if the measurement points at another block/symptom, say so; do not conflate a real bug found in passing with the cause being chased.

---

## LAW — an audio bug is found and proven by an AUTOMATED TEST. NEVER by ear.

**FORBIDDEN to ask the owner to listen and confirm ("run it and tell me if the click is gone").** He refused, emphatically: ear tests regress later, and finding and proving the fix is the engineer's job. Finding the defect AND proving the fix is the agent's work, through a deterministic test — not the owner's ear.

**Why:** an ear is not a test — it is not deterministic, leaves no regression guard, and pushes the engineer's job onto the user. A bug "confirmed by ear" comes back silently.

**How to apply (audio / real-time / scheduling):** reduce the defect to a property a test asserts **with no hardware and no listening**:
- **Pure logic:** `BudgetTracker` (`crates/infra-cpal/src/budget_tracker.rs`, RT budget churn), block DSP math, routing, mixdown → a unit test on the function itself.
- **Signal property** of the rendered buffer: NaN/Inf, a click (a sample-to-sample jump above the band limit), a hard-clip run, DC, level → `crates/engine/src/audio_signal_integrity_tests.rs`.
- **Counters/accounting:** assert the invariant (e.g. an overload MUST bump a surfaced counter), not the sound.
- **When the damage is timing** (worker stall, late buffer), test the LOGIC that triggers it (e.g. the budget re-declaring on a transient wall-clock spike), which is deterministic, instead of the flaky wall-clock magnitude.
- The hardware battery (`OPENRIG_HW_TESTS=1`) is for the AGENT to run and observe — it never replaces the deterministic guard and is never judged by the owner's ear.

---

## LAW — never hardcode the sample rate (or any device-dependent value)

**No audio/analysis path may assume a fixed sample rate.** Each interface runs at whatever rate suits it (44.1k, 48k, 96k…). Every rate-dependent calculation — pitch (tuner), FFT bins → Hz (spectrum), latency, period, loop resampling (DI), timing — **must use the REAL rate the stream negotiated**, never a literal.

**The source of truth for the live rate:**
- `ProjectRuntimeController::sample_rate()` — the rate the streams actually opened at (mirrored from `resolved.sample_rate`).
- `LocalDispatcher::engine_sr()` — the same rate, synced via `attach_engine_sr`.
- `resolve_input_sample_rate(project, device_id, live)` in `crates/adapter-gui/src/sample_rate.rs` — the one helper: the device's saved setting (authoritative; the stream is forced to it or fails) → otherwise the live rate. **Use it in the analysis consumers (tuner/spectrum/latency).** NEVER reimplement the resolution with `unwrap_or(48_000)`.

**The hardcode is almost never a loose literal — it is the FALLBACK.** The recurring mistake is `…device_settings…find(device)…map(|d| d.sample_rate).unwrap_or(48_000)`: when the device has no saved setting it hardcodes 48000 while the stream runs at 44.1k → everything reads ~1.47 semitones sharp (tuner: E becomes F; the DI loop in slow motion). An equally wrong variant: `.device_settings.iter().next()` (the first device) instead of THAT chain's/input's device.

**Why:** a hardcoded rate is a silent, hardware-dependent bug — it passes on the dev machine (48k) and breaks on the user's (44.1k). "The sound changed on macOS/Windows" = a regression (a `CLAUDE.md` red flag). It is the same disease that already bit the tuner, the spectrum, the latency probe and the DI loop.

**How to apply:** need a rate? Ask "where does this stream's LIVE rate come from?" — `controller.sample_rate()` / `dispatcher.engine_sr()` / `resolve_input_sample_rate`. Once a stream exists, the live rate ALWAYS exists; "I need some fallback value" is false on a live path. Legitimate exceptions (not hardcodes): a pre-device default overwritten on activation (the controller's initial state, the VST3 catalog init), render/CLI defaults the user overrides, the Linux/JACK fallback for an unconfigured device behind `cfg`, and a design compensation that USES the real variable (`scale = sample_rate / 44_100.0`).

| Excuse | Reality |
|---|---|
| "I need SOME value for the detector to run" | A live path always has a live rate: `controller.sample_rate()`/`engine_sr()`. The hardcoded fallback is the bug. |
| "48000 is the project default, it's safe" | A pre-device default ≠ a live stream's rate. On an analysis path a fixed 48000 wrecks anyone running 44.1k. |
| "`unwrap_or(default_sample_rate())` solves it" | Still a hardcode on the live path. The authoritative rate is the stream's, not a constant. |
| "It's just the first device (`.next()`)" | It must be THAT chain's/input's device. The first device is another stream/rate. |
| "It's cosmetic (a beep/a curve), it doesn't affect the sound" | Still a hardcode: extract a helper that takes the real rate. No 48000 planted. |

**Red flags — STOP:**
- `unwrap_or(48_000)` / `unwrap_or(44_100)` in code that reads live samples or computes Hz/latency/period.
- A `48_000`/`44_100` literal in a production `.rs` outside: a pre-device default const, `cfg(linux)` JACK, or a design ratio with the real variable.
- Re-deriving the rate yourself instead of calling `resolve_input_sample_rate` / `controller.sample_rate()`.
- `device_settings.iter().next()` to find "the rate".

---

## Audio runtime / DSP facts (hard-won — verify before touching these areas)

- **`ChainRuntimeState` locks:** the audio thread takes `processing.try_lock()` in `process_input_f32` and emits a SILENT buffer on failure. Any accessor the GUI calls repeatedly (meter timer at 30 Hz, spectrum, tuner) must NEVER take `processing.lock()` — mirror the value as an atomic (`AtomicUsize` etc.) updated at the rare write sites (`build_chain_runtime_state` + the rebuild path in `runtime_graph.rs`). Symptom of a violation is buffer-size dependent (32–64 glitches, 256+ absorbs); offline single-threaded tests pass while production clicks. Pinned by `crates/engine/src/stream_count_contention_tests.rs`.
- **Cabinet IR = `crates/ir`:** the CAB block is `block_ir` uniformly-partitioned FFT convolution, NOT the NAM C++ `dsp::ImpulseResponse` path. `ir::PARTITION_SIZE` must stay ≤ the smallest supported device buffer (64) or the per-partition FFT burst lands in one callback and xruns ("clips at 64, fine at 128"). Clamp `accum[0].im` and Nyquist `.im` to 0 before the inverse FFT (`realfft` panics on round-off there). The cold-start cushion is decoupled: `engine::IR_COLD_START_CUSHION_FRAMES`.
- **NAM is built from source:** `crates/nam/build.rs` extracts the vendored NeuralAmpModelerCore (`deps/NeuralAmpModelerCore.tar.gz` in Git LFS, pinned by `deps/NeuralAmpModelerCore.lock`) and builds the C wrapper in `cpp/` (`nam_wrapper`, `nam_tone_stack`) with CMake. Rust sees only `crates/nam/src/ffi.rs`; `NamPluginConfig` (`crates/nam/src/plugin_config.rs`) mirrors the C struct in `cpp/nam_wrapper.h` field for field — change both together. A NAM knob reaches the DSP only through a field of that struct (input/output level, noise gate, bass/middle/treble, IR, slim size).
- **Native blocks are pure-Rust DSP:** every `native_*` block compiles into the binary and needs no plugin package — a native that misbehaves is a code bug, never a missing file.
- **NAM captures live in `plugins_path`:** debug "NAM silent/not working" against `paths.plugins_path` from `config.yaml` (the owner's OpenRig-plugins checkout), not against the small test fixtures under `crates/engine/tests/fixtures/plugins/nam/`.

## GUI ↔ runtime wiring traps

- **Two block-creation paths:** `Command::AddBlock` → `block_factory::build_default_block` (`crates/application/src/block_factory.rs`), AND the GUI block editor → builds the block itself → `Command::InsertPrebuiltBlock` (never calls `build_default_block`). Param/manifest seeding must go through `block_factory::default_params_for_model`, which BOTH paths call — fixing only `build_default_block` leaves GUI-added blocks broken. Seed into the user-visible knob (visible, editable, persisted), never a silent load-time default.
- **Dispatch alone is dead:** a `Command` targeting a `ChainRuntimeState` only records intent + emits an `Event`. The GUI callback must ALSO apply the runtime effect inline right after dispatch (mirror `wire_mute_inline` in `tuner_wiring.rs`). Always add an end-to-end test driving the callback path and asserting the runtime state flipped — applier-only unit tests hide the gap.
- **"Doesn't resize / doesn't fit / not updating" UI bugs:** the logic usually exists but a code path doesn't TRIGGER it (e.g. the model-picker path rebuilt params without calling `apply_panel_dimensions`). Reproduce in the running app first; when told "do the same as X", mirror X's trigger, don't reinvent the calculation. NAM amps use the PANEL editor, generic/VST3 the FORM editor (`use_panel_editor` in `crates/project/src/catalog_listing.rs`) — don't assume which. Pin GUI-wiring fixes with source-presence tests (the `crates/adapter-gui/tests/no_native_dialogs.rs` convention).
