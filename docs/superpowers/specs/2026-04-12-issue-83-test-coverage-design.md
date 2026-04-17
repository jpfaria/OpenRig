# Issue #83 — Test Coverage Design

## Goal

100% test coverage on all testable code across the entire OpenRig project. Prevent regressions that have caused production crashes (RefCell panics, audio glitches, YAML serialization issues).

## Decisions

| Decision | Choice |
|----------|--------|
| Coverage tool | `cargo-llvm-cov` |
| CI behavior | Informative (no merge gate) |
| Target | 100% of testable code |
| DSP testing | Unitarios rapidos + integracao com audio real (`#[ignore]`) |
| Approach | Hibrida: base (block-core) → risco alto → runtime → DSP → GUI |

---

## Section 1: Coverage Infrastructure & CI

### Local

- `cargo-llvm-cov` installed via `cargo install cargo-llvm-cov`
- `rustup component add llvm-tools-preview`
- Script `scripts/coverage.sh`: runs `cargo llvm-cov --workspace --html --output-dir coverage/`
- `.gitignore`: add `coverage/`

### CI (GitHub Actions)

New workflow `.github/workflows/test.yml`:
- **Trigger:** push and PR to `develop` and `feature/*`
- **Job:** Linux only
  - `cargo test --workspace`
  - `cargo llvm-cov --workspace --lcov --output-path lcov.info`
  - Upload report as artifact (informative, no gate)

---

## Section 2: Test Conventions

### Location
Tests inside the module itself: `#[cfg(test)] mod tests { ... }` — idiomatic Rust, already used in the project.

### Integration tests with real audio
Marked with `#[ignore]`. Run manually: `cargo test -- --ignored`.

### Naming
Pattern: `<function_or_behavior>_<scenario>_<expected_result>`
Example: `validate_project_rejects_empty_chains`, `elastic_buffer_underrun_repeats_last_frame`

### Test helpers
Where multiple tests need complex structs (Chain, AudioBlock, Project), create `fn test_chain_with_*()` in the crate's test module. No separate test-utils crate — each crate is self-sufficient.

### Assertions
Use `assert!`, `assert_eq!`, `assert!(result.is_err())` — no external frameworks.

### Golden samples for DSP
Record current correct output as "golden" and compare with tolerance: `(actual - expected).abs() < 1e-4`. Detects numeric regressions.

---

## Section 3: Scope by Crate

### Layer 1 — Base (no complex internal dependencies)

**`domain`** (0 tests → 100%)
- Value object construction and validation: `ChainId`, `BlockId`, `ParameterId`, `Normalized`, `Db`, `Hertz`
- Type conversions, limits, edge cases

**`block-core`** (0 tests → 100%)
- `ModelAudioMode::accepts_input()` and `output_layout()` — core audio compatibility logic
- `BiquadFilter`, `EnvelopeFollower`, `OnePoleLowPass`, `OnePoleHighPass` — pure DSP, golden samples
- `ParameterSet::normalized_against()` — validation with defaults
- `ParameterSpec::validate_value()` — validation per domain type (float, bool, enum, file)
- Utility functions: `capitalize_first`, `db_to_lin`, `lin_to_db`, `calculate_coefficient`
- Parameter builders: `float_parameter`, `enum_parameter`, `bool_parameter`, etc.

### Layer 2 — High risk (recent crashes)

**`application`** (0 tests → 100%)
- `validate_project()` — empty chains, missing inputs/outputs, invalid device settings
- Channel conflict validation between active chains
- Layout propagation through block chain

**`infra-yaml`** (8 tests → expand)
- Roundtrip of ALL block types (some missing)
- Edge cases: empty project, chain without blocks, parameter boundary values
- Legacy format migration with Insert blocks
- Preset with unknown types

**`infra-filesystem`** (1 test → expand)
- `AssetPaths` — platform path resolution
- `AppConfig` — load/save configuration
- `RecentProjectEntry` — recent projects management
- `GuiAudioSettings` — audio settings persistence

### Layer 3 — Runtime

**`engine`** (26 tests → expand)
- `build_runtime_graph` with uncovered block combinations
- `update_chain_runtime_state` — hot-update with Insert blocks
- I/O processing with multiple inputs/outputs
- Select block switching at runtime
- `ElasticBuffer` edge cases (extreme drift, buffer size 1)

**`project`** (19 tests → expand)
- `Chain::validate_channel_conflicts()` — uncovered scenarios
- `Project::find_block()` — recursive search with nested Select
- `ProcessingLayout` — input mode combinations

### Layer 4 — DSP (block-*)

**For each crate (17 crates, ~400 models):**
- **Unit (all):** `schema()` returns valid schema, `validate()` accepts defaults, `validate()` rejects out-of-range values, `build()` constructs without panic for Mono and Stereo
- **Integration `#[ignore]` (native + IR):** process N frames of silence/sine, verify non-NaN output, compare with golden samples
- **NAM/LV2:** unit tests only (depend on external assets/binaries)

### Layer 5 — GUI

**`adapter-gui`** (23 tests → expand)
- `accent_color_for_icon_kind` — all 21 types
- `icon_index_for_icon_kind` — complete mapping
- `block_family_for_kind` — categorization
- `chain_routing_summary` — label formatting
- `build_multi_slider_points`, `build_curve_editor_points` — audio curves
- `plugin_metadata`, `thumbnail_png` — caching (OnceLock)
- EQ coordinate functions: `freq_to_x`, `gain_to_y`, `biquad_kind_for_group`

**Out of scope:** Slint callbacks, rendering, mouse/keyboard interactions (require GUI runtime).

### Other crates

**`infra-cpal`** (4 tests → expand where testable)
- Device enumeration mocking where possible
- Configuration validation

**`nam`**, **`ir`** (0-2 tests → expand)
- `ir`: WAV loading, mono/stereo processor construction
- `nam`: processor construction (requires model files — `#[ignore]`)

**`lv2`** (0 tests)
- Plugin loading validation (requires .so/.dylib — `#[ignore]`)

**Empty crates** (`state`, `preset`, `ports`) — skip, no implementation.

---

## Documentation Updates

When implementation is complete:
- Update `openrig-code-quality` skill with testing rules section
- Update `CLAUDE.md` with test conventions and coverage tooling
