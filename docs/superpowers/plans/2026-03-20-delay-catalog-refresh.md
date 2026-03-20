# Delay Catalog Refresh Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Replace the old public delay models with a new six-model product catalog and break compatibility with the old names.

**Architecture:** Keep `block-delay` as the single crate for delay models, reuse the current DSP where it makes sense, and move the public contract to the new names only. Projects using old delay names should fail validation explicitly.

**Tech Stack:** Rust, existing `block-delay` crate, `project`/`infra-yaml` contract, cargo test/check.

---

### Task 1: Replace delay model catalog

**Files:**
- Modify: `crates/block-delay/src/lib.rs`
- Modify: `crates/block-delay/src/digital_basic.rs`
- Create/Modify: `crates/block-delay/src/*`
- Test: `crates/block-delay/src/*`

- [ ] Add failing tests for the new public model names.
- [ ] Make old names stop resolving publicly.
- [ ] Implement the six-model catalog.
- [ ] Run delay tests.

### Task 2: Update project/runtime contract

**Files:**
- Modify: `crates/project/src/block.rs`
- Modify: `crates/infra-yaml/src/lib.rs`
- Modify: `project.yaml`
- Modify: `presets/example.yaml`

- [ ] Add/adjust tests so old delay names are rejected.
- [ ] Update examples to new names.
- [ ] Verify contract and YAML loading.

### Task 3: Verify non-UI impact

**Files:**
- Modify as needed: `crates/application`, `crates/engine`

- [ ] Run targeted `cargo test`.
- [ ] Run targeted `cargo check`.
- [ ] Run targeted `cargo clippy`.
