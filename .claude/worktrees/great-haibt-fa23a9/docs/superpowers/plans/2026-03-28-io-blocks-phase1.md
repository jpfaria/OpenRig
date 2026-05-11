# I/O Blocks Phase 1 — Input and Output as blocks in the chain

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Move Input/Output from separate lists (`Chain.inputs`/`Chain.outputs`) into `chain.blocks` as `AudioBlockKind::Input` and `AudioBlockKind::Output` variants.

**Architecture:** I/O blocks live in `chain.blocks` alongside effect blocks. First block must be Input (fixed), last must be Output (fixed). Extra I/O blocks can be added in the middle. Each Input creates an isolated parallel stream. Output is a tap (copies signal, doesn't interrupt). The chain view keeps current In/Out chip visuals.

**Branch:** `feature/issue-79`

---

## Task 1: Add InputBlock/OutputBlock to data model

**Files:**
- Modify: `crates/project/src/block.rs`
- Modify: `crates/project/src/chain.rs`

### Steps:
- [ ] Add InputBlock and OutputBlock structs to block.rs
- [ ] Add Input(InputBlock) and Output(OutputBlock) variants to AudioBlockKind
- [ ] Update all match statements in AudioBlock methods (validate_params, parameter_descriptors, audio_descriptors, model_ref)
- [ ] Update SelectBlock validation to reject Input/Output as options
- [ ] Remove `Chain.inputs`, `Chain.outputs` and all related structs (ChainInput, ChainOutput)
- [ ] Remove `migrate_legacy_io()`, `validate_channel_conflicts()`, `processing_layout_for_input()`
- [ ] Add `Chain.input_blocks()` and `Chain.output_blocks()` helper methods that filter blocks by kind
- [ ] Add `Chain.first_input()` and `Chain.last_output()` helpers
- [ ] Add validation: chain must start with Input block and end with Output block
- [ ] `cargo build` — fix all compilation errors in other crates
- [ ] `cargo test` — fix broken tests
- [ ] Commit

## Task 2: Update YAML serialization and migration

**Files:**
- Modify: `crates/infra-yaml/src/lib.rs`
- Modify: `project.yaml`
- Modify: `~/.openrig/project.yaml`

### Steps:
- [ ] Update ChainYaml — remove ChainInputYaml/ChainOutputYaml, blocks now include I/O blocks
- [ ] Add YAML serialization for InputBlock/OutputBlock (type: "input"/"output" in blocks list)
- [ ] Update `into_chain()` — legacy migration: if chain has old `input_device_id` but no Input block in blocks, prepend Input block; if no Output block, append Output block
- [ ] Update `from_chain()` — serialize I/O blocks inline in blocks list
- [ ] Update `project.yaml` in repo root — convert inputs/outputs to Input/Output blocks in blocks list
- [ ] Update `~/.openrig/project.yaml` — same conversion
- [ ] `cargo build` + `cargo test`
- [ ] Commit

## Task 3: Update engine runtime

**Files:**
- Modify: `crates/engine/src/runtime.rs`

### Steps:
- [ ] Remove `effective_inputs()` / `effective_outputs()` — no longer needed
- [ ] Update `build_chain_runtime_state()` — scan `chain.blocks` to find Input/Output blocks, build InputProcessingState for each Input, OutputRoutingState for each Output
- [ ] For each Input block: blocks AFTER it (until next Input or end) are its processing chain
- [ ] Input block at position N: its block chain = blocks[N+1..] that are not Input blocks (effect blocks only)
- [ ] Output blocks in the chain are taps — when processing reaches an Output's position, copy current frame to that Output's queue
- [ ] Ensure each Input has completely isolated state (own blocks, own frame_buffer)
- [ ] Update `process_input_f32()` — no changes needed if InputProcessingState is built correctly
- [ ] Update `process_output_f32()` — no changes needed if OutputRoutingState is built correctly
- [ ] Update `update_chain_runtime_state()` — rebuild from blocks
- [ ] Handle edge case: blocks between two Inputs are only processed by the first Input's stream
- [ ] `cargo build` + `cargo test`
- [ ] Commit

## Task 4: Update CPAL stream building

**Files:**
- Modify: `crates/infra-cpal/src/lib.rs`

### Steps:
- [ ] Update `resolve_chain_inputs()` — scan chain.blocks for Input blocks instead of chain.inputs
- [ ] Update `resolve_chain_outputs()` — scan chain.blocks for Output blocks instead of chain.outputs
- [ ] Rest of CPAL should work since it already creates per-input/per-output streams
- [ ] `cargo build` + `cargo test`
- [ ] Commit

## Task 5: Update GUI

**Files:**
- Modify: `crates/adapter-gui/src/lib.rs`
- Modify: `crates/adapter-gui/ui/pages/chain_editor.slint`
- Modify: `crates/adapter-gui/ui/app-window.slint`

### Steps:
- [ ] Remove InputGroupDraft/OutputGroupDraft — I/O is now in blocks
- [ ] Update ChainDraft — remove inputs/outputs fields
- [ ] Update chain_from_draft() — I/O blocks are part of blocks list
- [ ] Update chain_draft_from_chain() — extract I/O info from blocks
- [ ] Update chain_editor.slint — keep showing I/O groups section but populate from blocks
- [ ] Update on_configure_chain_input/output — work with I/O blocks
- [ ] Update tooltips — extract I/O info from blocks
- [ ] `cargo build`
- [ ] Commit

## Task 6: Update docs and cleanup

**Files:**
- Modify: `CLAUDE.md`
- Remove: unused I/O group types if any

### Steps:
- [ ] Update CLAUDE.md
- [ ] Final `cargo build` + `cargo test` — zero warnings
- [ ] Commit
- [ ] Create PR
