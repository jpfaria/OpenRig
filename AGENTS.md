These instructions apply to the entire repository unless a deeper `AGENTS.md` overrides them.

## Branch Naming (MANDATORY — read before creating ANY branch)

**Format: `feature/issue-{N}` or `bugfix/issue-{N}` — NOTHING ELSE.**

- NEVER add timestamps: ~~`feature/issue-99-20260402-0549`~~ → `feature/issue-99`
- NEVER add descriptions: ~~`feature/issue-99-harmonizer`~~ → `feature/issue-99`
- NEVER add suffixes: ~~`feature/issue-99-v2`~~ → `feature/issue-99`
- Before creating a branch, ALWAYS check if one already exists:
  ```bash
  git fetch origin && git branch -a | grep "issue-{N}"
  ```
- If a branch exists, use it. NEVER create a second branch for the same issue.
- This repo is a Rust workspace with multiple crates under `crates/`; do not describe it or treat it as a single-crate project.
- Keep changes aligned with the current workspace layout unless the user explicitly asks for a structural migration.
- Current crate families are:
- `crates/domain`, `crates/project`, `crates/application`, `crates/engine`
- `crates/infra-yaml`, `crates/infra-filesystem`, `crates/infra-cpal`
- `crates/adapter-console`, `crates/adapter-server`, `crates/adapter-gui`, `crates/adapter-vst3`
- `crates/block-*` for DSP stage implementations and shared stage utilities
- Runtime project data is currently loaded from `project.yaml`, and app configuration lives in `config.yaml`.
- `project.yaml` is the runtime source of truth for a project.
- The sibling `config.yaml` next to a project is project-local configuration, currently used for `presets_path`.
- Adapter-global persistence under `~/.config/OpenRig/` is adapter state, not part of the runtime project model.
- Native NAM integration is built in `crates/nam/build.rs` and linked through the `cpp/` CMake project.
- Treat this project as core-first.
- Keep `domain`, `application`, and `engine` conceptually independent from YAML, filesystem, device backends, and UI/adapter concerns.
- Preserve clear boundaries between domain models, orchestration/use cases, runtime audio processing, infrastructure adapters, and presentation adapters.
- Consider future adapters such as console, server, GUI, and VST3, but do not force large migrations unless requested.
- `crates/domain`: ids and value objects only; no YAML, device I/O, filesystem, audio backend, or UI concerns.
- `crates/project`: project models, chains, device settings, and audio block definitions.
- `crates/application`: orchestration, validation, commands, and use-case logic.
- `crates/engine`: runtime graph construction and audio processing behavior.
- `crates/infra-*`: YAML loading, filesystem access, CPAL integration, persistence adapters, and other external integrations.
- `crates/adapter-*`: entrypoints and delivery adapters; do not let console or transport concerns shape domain models.
- `crates/block-*`: DSP implementations and block-specific processing primitives.
- Do not treat the system as YAML-first. YAML is one adapter for loading project data and presets.
- Do not reason about the current system as `setup`/`state`/`preset` driven. The runtime model is `Project -> Chain -> AudioBlock`.
- Chains own their `blocks` directly. Preset files are import/export artifacts for replacing a chain's blocks; chains do not reference presets at runtime.
- User-authored `project.yaml` does not provide chain IDs or block IDs. These are generated internally at load time.
- Model the pedalboard around generic audio blocks, not only NAM.
- Treat NAM, IR, and internal effects as separate concepts even when combined in runtime flows.
- Prefer abstractions that allow internal blocks such as delay, reverb, tremolo, EQ, dynamics, routing, selectors, amp, and cab stages.
- File-based YAML loading belongs in infrastructure, not in domain or application crates.
- If project or preset persistence evolves, prefer adapters over hard-wiring storage into domain or application logic.
- Avoid introducing direct coupling from application logic to YAML, filesystem, database, RPC, or UI clients.
- When changing project schema or semantics, update loader and validation together.
- Keep `project.yaml` and `config.yaml` compatibility in sync with the corresponding models.
- Persist audio devices by backend `device_id`, not `match_name`.
- `device_settings` is optional project-level override data keyed by `device_id`.
- Active chains may not share the same `input_device_id + input_channel`; disabled chains are ignored by that conflict rule.
- Chain and block `enabled` flags must be respected consistently by validation and runtime.
- Current routing/layout support is mono or stereo only.
- For backend decisions, prefer `docs/backend/current-contract.md` and `docs/adr/` as the canonical reference before relying on old chat context.
- For MK-300-inspired native block work, prefer `docs/backend/mk-300-v69-effects-reference.md` for canonical parameter naming and module/type grouping.
- For native block target selection and implementation order, prefer `docs/backend/native-model-catalog.md` before inventing new models or re-triaging the capture library.
- Device names and model paths are machine-specific; avoid baking in new local paths unless the task explicitly requires it.
- Start validation with `cargo check` for Rust changes.
- Changes touching NAM integration should review `crates/nam` Rust FFI code, `crates/nam/build.rs`, and `cpp/` inputs together.
- On macOS, native/audio verification may depend on CoreAudio-related frameworks, local toolchain state, and attached audio hardware.
- For native or audio changes, state clearly when verification is limited by OS, toolchain, hardware, or device availability.
- Make minimal, targeted changes.
- Preserve existing naming and crate boundaries unless the user asks for a refactor.
- Avoid editing vendored sources under `NeuralAmpModelerCore/Dependencies` unless explicitly required.
- Do not rely on generated files under `target/`.
- Clean obvious unused imports or warnings introduced by your changes.
- Tolerate temporary `dead_code` in broader domain modeling only when it supports the intended architecture.

## I/O Blocks Architecture

- `InputBlock`, `OutputBlock`, and `InsertBlock` are `AudioBlockKind` variants stored in `chain.blocks`.
- The first block in a chain is always a fixed Input; the last block is always a fixed Output.
- Extra I/O blocks (Input, Output, Insert) can be placed anywhere in the middle of the chain.
- Each Input creates an isolated parallel audio stream (independent processing instance).
- Output is a non-destructive tap — it copies the signal without interrupting the chain flow.
- Insert splits the chain into segments — each segment has its own effect blocks and output routes. When disabled, the signal bypasses (passes through).
- `InputBlock` and `OutputBlock` have a `model` field (default "standard") and `entries` (Vec of InputEntry/OutputEntry). The `name` lives on each entry, not on the block itself.
- `InsertBlock` has `model`, `send: InsertEndpoint`, and `return_: InsertEndpoint`.
- **Save behavior**: updating an I/O block only saves that specific block — it never reconstructs the entire chain.

## Task Execution Strategy

Before implementing any issue, evaluate its complexity:

### Step 1 — Assess complexity
- Read the full issue, including all comments
- Estimate how many files need to change
- If more than 5 files or 3 logical changes: the task is complex

### Step 2 — Plan before coding
- For complex tasks: post a plan as a comment listing numbered sub-tasks
- Each sub-task must be small, independently compilable, and committable
- Wait for approval before implementing (unless the user said "implement" or "execute")

### Step 3 — Execute sequentially
- Work on ONE sub-task at a time
- After each sub-task: `cargo build` must pass with zero warnings
- Commit after each sub-task with a clear message
- Use a SINGLE branch for the entire issue: `feature/issue-{N}` or `bugfix/issue-{N}` — NO suffixes, NO timestamps, NO descriptions after the number

### Step 4 — Deliver
- When all sub-tasks are done, create a single PR to `develop`
- PR body must include `Closes #N` where N is the issue number
- List what was done in the PR description

### If a run is interrupted (max-turns)
- Comment on the issue with progress so far and what remains
- On the next `@claude continue`, pick up from where you left off
