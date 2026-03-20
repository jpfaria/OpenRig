# Native Amp, Cab, and Combo Models Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Add 9 native guitar models to the core runtime: 3 amp-heads, 3 cabs, and 3 amp-combos.

**Architecture:** Keep one shared DSP core per family instead of 9 isolated implementations. The public contract lives in `block-amp-head`, `block-cab`, and `block-amp-combo`; each family exposes 3 native voicings with a consistent parameter set and routes builds through shared native processors. Project/runtime integration should work without UI changes.

**Tech Stack:** Rust, existing `block-core` DSP helpers, crate-level model registries, `cargo test`, `cargo check`, `cargo clippy`.

---

### Task 1: Lock The Public Contract With Tests

**Files:**
- Modify: `/Users/joao.faria/Projetos/github.com/jpfaria/OpenRig/crates/block-amp-head/src/lib.rs`
- Modify: `/Users/joao.faria/Projetos/github.com/jpfaria/OpenRig/crates/block-cab/src/lib.rs`
- Modify: `/Users/joao.faria/Projetos/github.com/jpfaria/OpenRig/crates/block-amp-combo/src/lib.rs`
- Modify: `/Users/joao.faria/Projetos/github.com/jpfaria/OpenRig/crates/project/src/block.rs`

- [ ] **Step 1: Write failing contract tests for the 9 new models**

Cover:
- amp-head models: `brit_crunch_head`, `american_clean_head`, `modern_high_gain_head`
- cab models: `brit_4x12_cab`, `american_2x12_cab`, `vintage_1x12_cab`
- amp-combo models: `blackface_clean_combo`, `tweed_breakup_combo`, `chime_combo`
- parameter schema expectations for each family
- project contract acceptance for at least one model per family

- [ ] **Step 2: Run targeted tests to verify they fail for the expected reason**

Run:
```bash
cargo test -p block-amp-head -p block-cab -p block-amp-combo -p project
```

Expected:
- failures because the new models are not wired yet

### Task 2: Implement Shared Native DSP Cores

**Files:**
- Create: `/Users/joao.faria/Projetos/github.com/jpfaria/OpenRig/crates/block-amp-head/src/native.rs`
- Create: `/Users/joao.faria/Projetos/github.com/jpfaria/OpenRig/crates/block-cab/src/native.rs`
- Create: `/Users/joao.faria/Projetos/github.com/jpfaria/OpenRig/crates/block-amp-combo/src/native.rs`
- Modify: `/Users/joao.faria/Projetos/github.com/jpfaria/OpenRig/crates/block-amp-head/src/lib.rs`
- Modify: `/Users/joao.faria/Projetos/github.com/jpfaria/OpenRig/crates/block-cab/src/lib.rs`
- Modify: `/Users/joao.faria/Projetos/github.com/jpfaria/OpenRig/crates/block-amp-combo/src/lib.rs`

- [ ] **Step 1: Implement native amp-head core**

Requirements:
- mono/dual-mono head processing
- shared parameter set:
  - `input_db`, `gain`, `bass`, `middle`, `treble`, `presence`, `depth`, `master`, `output_db`, `bright`, `sag`
- 3 voicings with distinct internal coefficients/drive behavior

- [ ] **Step 2: Implement native cab core**

Requirements:
- mono and stereo support
- shared parameter set:
  - `low_cut_hz`, `high_cut_hz`, `resonance`, `air`, `mic_position`, `mic_distance`, `room_mix`, `output_db`
- 3 cab voicings using distinct filter/resonance profiles

- [ ] **Step 3: Implement native amp-combo core**

Requirements:
- shared parameter set:
  - `input_db`, `gain`, `bass`, `middle`, `treble`, `master`, `bright`, `sag`, `room_mix`, `output_db`
- combine native amp voicing + native cab voicing + combo-specific tuning

### Task 3: Wire Registries And Runtime Builders

**Files:**
- Modify: `/Users/joao.faria/Projetos/github.com/jpfaria/OpenRig/crates/block-amp-head/src/lib.rs`
- Modify: `/Users/joao.faria/Projetos/github.com/jpfaria/OpenRig/crates/block-cab/src/lib.rs`
- Modify: `/Users/joao.faria/Projetos/github.com/jpfaria/OpenRig/crates/block-amp-combo/src/lib.rs`
- Modify: `/Users/joao.faria/Projetos/github.com/jpfaria/OpenRig/crates/project/src/block.rs`

- [ ] **Step 1: Register the new models in each family**

Requirements:
- schema lookup works
- validation works
- processor builders work
- backend kind reports `Native` where appropriate

- [ ] **Step 2: Keep existing NAM/IR-backed models working**

Requirements:
- do not break `marshall_jcm_800_2203`
- do not break `marshall_4x12_v30`
- do not break `bogner_ecstasy`

### Task 4: Verify The Core End-To-End

**Files:**
- No new files required

- [ ] **Step 1: Run check**

Run:
```bash
cargo check -p block-amp-head -p block-cab -p block-amp-combo -p project -p engine -p application
```

- [ ] **Step 2: Run tests**

Run:
```bash
cargo test -p block-amp-head -p block-cab -p block-amp-combo -p project -p engine -p application
```

- [ ] **Step 3: Run clippy**

Run:
```bash
cargo clippy -p block-amp-head -p block-cab -p block-amp-combo -p project -p engine -p application --all-targets -- -D warnings
```

- [ ] **Step 4: Commit**

```bash
git add docs/superpowers/plans/2026-03-20-native-amp-cab-combo-models.md \
  crates/block-amp-head \
  crates/block-cab \
  crates/block-amp-combo \
  crates/project
git commit -m "feat: add native amp cab and combo models"
```
