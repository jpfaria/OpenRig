# Chain-Level Runtime — Play/Stop per Chain

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Remove project-level play/stop. Each chain starts/stops independently. Chains always start disabled on load.

**Architecture:** The ProjectRuntimeController already supports per-chain upsert/remove. The change is: (1) remove the project play/stop UI and callbacks, (2) make the chain power toggle start/stop the runtime for that chain directly (creating the runtime if needed), (3) stop persisting chain.enabled in YAML — always load as false.

**Tech Stack:** Rust, Slint, CPAL

---

## Files to modify

| File | Change |
|------|--------|
| `crates/adapter-gui/ui/pages/project_chains.slint` | Remove play/stop buttons from header, remove `project-running` property |
| `crates/adapter-gui/src/lib.rs` | Remove `on_start_project`/`on_stop_project`, change `on_toggle_chain_enabled` to create runtime on demand |
| `crates/infra-yaml/src/lib.rs` | Stop persisting `enabled` for chains (always load as `false`) |
| `crates/project/src/chain.rs` | No change to struct — `enabled` stays as runtime state, just not persisted |
| `crates/adapter-gui/ui/app-window.slint` | Remove `project-running` if used |

---

### Task 1: Stop persisting chain.enabled in YAML

**Files:**
- Modify: `crates/infra-yaml/src/lib.rs`

- [ ] **Step 1: Change ChainYaml deserialization to always set enabled=false**

In `ChainYaml.into_chain()` (line ~202), replace:
```rust
enabled: self.enabled,
```
with:
```rust
enabled: false, // chains always start disabled
```

- [ ] **Step 2: Remove enabled from ChainYaml serialization**

In `ChainYaml::from_chain()` (line ~222), stop writing `enabled`. Add `#[serde(skip_serializing)]` to the `enabled` field in `ChainYaml` struct (line ~185).

- [ ] **Step 3: Compile and test**

```bash
cargo build
cargo test -p infra-yaml
```

- [ ] **Step 4: Commit**

```bash
git commit -m "feat: chains always start disabled, enabled not persisted in YAML"
```

---

### Task 2: Make chain toggle create runtime on demand

**Files:**
- Modify: `crates/adapter-gui/src/lib.rs`

- [ ] **Step 1: Change on_toggle_chain_enabled to create runtime if needed**

Currently `sync_live_chain_runtime` (line ~5329) returns early if `project_runtime` is None. Change it: if runtime is None and chain is being enabled, CREATE the runtime first.

In `on_toggle_chain_enabled` (line ~3906), after `chain.enabled = !chain.enabled`:
- If `chain.enabled == true` and runtime is None → create runtime via `ProjectRuntimeController::start()`
- Then call `sync_live_chain_runtime` as before

- [ ] **Step 2: Ensure disabling last chain stops runtime**

After `sync_live_chain_runtime`, check if runtime has no active chains → set to None.

- [ ] **Step 3: Compile and test**

```bash
cargo build
```

- [ ] **Step 4: Commit**

```bash
git commit -m "feat: chain toggle creates/destroys runtime on demand"
```

---

### Task 3: Remove project-level play/stop from UI

**Files:**
- Modify: `crates/adapter-gui/ui/pages/project_chains.slint`
- Modify: `crates/adapter-gui/src/lib.rs`

- [ ] **Step 1: Remove play/stop buttons from header**

In `project_chains.slint`, remove the two `HeaderIconAction` blocks for play/stop (lines ~1858-1874).

- [ ] **Step 2: Remove start-project/stop-project callbacks from ProjectChainsPage**

Remove `callback start-project()` and `callback stop-project()` declarations.

- [ ] **Step 3: Remove project-running property**

Remove `in property <bool> project-running` and any references to it.

- [ ] **Step 4: Remove on_start_project/on_stop_project handlers from lib.rs**

Remove `window.on_start_project(...)` (lines ~3941-3961) and `window.on_stop_project(...)` (lines ~3967-3974).

- [ ] **Step 5: Remove start_project_runtime/stop_project_runtime functions**

Remove `fn start_project_runtime()` (line ~5312) and `fn stop_project_runtime()` (line ~5306). Keep `sync_live_chain_runtime` — it's still needed.

- [ ] **Step 6: Clean up set_project_running calls**

Remove all `window.set_project_running(...)` calls throughout lib.rs.

- [ ] **Step 7: Compile and test**

```bash
cargo build
```

- [ ] **Step 8: Commit**

```bash
git commit -m "feat: remove project-level play/stop, runtime is per-chain"
```

---

### Task 4: Update chain display to show running state

**Files:**
- Modify: `crates/adapter-gui/ui/pages/project_chains.slint`
- Modify: `crates/adapter-gui/src/lib.rs`

- [ ] **Step 1: ChainPowerToggle already works**

The existing `ChainPowerToggle` already shows green when `chain.enabled == true`. Since `toggle_chain_enabled` now starts the runtime, the visual feedback is already correct.

- [ ] **Step 2: Update latency badge**

The latency badge should only show when the chain is running (enabled). It already uses `chain.latency_ms > 0` which depends on device settings. No change needed.

- [ ] **Step 3: Verify all chains start disabled on app launch**

Open the app, verify no chains are playing, verify clicking power toggle starts audio for that chain only.

- [ ] **Step 4: Commit**

```bash
git commit -m "feat: verify chain-level runtime works end-to-end"
```

---

## Verification

1. Open the app — no chains should be playing (all power toggles off)
2. Click power toggle on one chain — only that chain should start processing audio
3. Click power toggle on another chain — both should be running independently
4. Disable a chain — only that chain stops, the other keeps running
5. Disable all chains — runtime should be fully stopped
6. Save and reload project — all chains should be disabled again
7. No play/stop button in the header
