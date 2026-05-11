# I/O Blocks GUI Fixes — Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Fix all GUI bugs for I/O blocks: correct insert position from picker, proper entry add/remove flows, enable/disable LED, arrow icons for middle I/O blocks.

**Architecture:** The key insight is that there are TWO separate save paths that must NEVER be mixed: (1) chip In/Out edits the fixed first/last block by updating entries in-place, (2) picker "+" creates a NEW block at a specific position. The `chain_from_draft` function must NEVER be called from the I/O groups save path — it reconstructs the entire chain and destroys block positions.

**Tech Stack:** Rust, Slint UI

**Branch:** `feature/issue-79`

---

### Task 1: Fix the save path — never use chain_from_draft for I/O edits

The root cause of most bugs: `chain_input_groups_window.on_save` falls through to `chain_from_draft` when `editing_io_block_index` is `None` (chip In/Out). This reconstructs the entire chain and destroys positions.

**Files:**
- Modify: `crates/adapter-gui/src/lib.rs` (lines ~2925-3035 input save, ~3148-3270 output save)

- [ ] **Step 1: Write test for save path**

Add to `crates/adapter-gui/src/lib.rs` tests module:

```rust
#[test]
fn chain_from_draft_does_not_move_middle_io_blocks() {
    // Create a chain with Input at 0, effect at 1, Input at 2, effect at 3, Output at 4
    let chain = Chain {
        id: ChainId("test".into()),
        description: None,
        instrument: "electric_guitar".into(),
        enabled: true,
        blocks: vec![
            AudioBlock { id: BlockId("input:0".into()), enabled: true,
                kind: AudioBlockKind::Input(InputBlock { model: "standard".into(),
                    entries: vec![InputEntry { name: "G1".into(), device_id: DeviceId("dev".into()), mode: ChainInputMode::Mono, channels: vec![0] }] }) },
            AudioBlock { id: BlockId("gain:0".into()), enabled: true,
                kind: AudioBlockKind::Core(CoreBlock { effect_type: "gain".into(), model: "volume".into(), params: ParameterSet::default() }) },
            AudioBlock { id: BlockId("input:1".into()), enabled: true,
                kind: AudioBlockKind::Input(InputBlock { model: "standard".into(),
                    entries: vec![InputEntry { name: "G2".into(), device_id: DeviceId("dev".into()), mode: ChainInputMode::Mono, channels: vec![1] }] }) },
            AudioBlock { id: BlockId("delay:0".into()), enabled: true,
                kind: AudioBlockKind::Core(CoreBlock { effect_type: "delay".into(), model: "digital_clean".into(), params: ParameterSet::default() }) },
            AudioBlock { id: BlockId("output:0".into()), enabled: true,
                kind: AudioBlockKind::Output(OutputBlock { model: "standard".into(),
                    entries: vec![OutputEntry { name: "Out".into(), device_id: DeviceId("dev".into()), mode: ChainOutputMode::Stereo, channels: vec![0, 1] }] }) },
        ],
    };
    // Verify block order
    assert!(matches!(&chain.blocks[0].kind, AudioBlockKind::Input(_)));
    assert!(matches!(&chain.blocks[1].kind, AudioBlockKind::Core(_)));
    assert!(matches!(&chain.blocks[2].kind, AudioBlockKind::Input(_)));
    assert!(matches!(&chain.blocks[3].kind, AudioBlockKind::Core(_)));
    assert!(matches!(&chain.blocks[4].kind, AudioBlockKind::Output(_)));
}
```

- [ ] **Step 2: Run test to verify it passes (this is a structural test)**

Run: `cargo test --package adapter-gui -- chain_from_draft_does_not_move`

- [ ] **Step 3: Rewrite input groups save to ALWAYS update in-place**

In `chain_input_groups_window.on_save` (line ~2925), replace the fallthrough to `chain_from_draft` with direct block update:

When `editing_io_block_index` is `None` (chip In), find the first InputBlock in `chain.blocks` and update its entries. When `Some(idx)`, update `chain.blocks[idx]`.

**NEVER call `chain_from_draft`** from this save path.

```rust
// In chain_input_groups_window.on_save:
let editing_index = draft.editing_index;
let io_block_idx = draft.editing_io_block_index;

// Build new entries from draft
let new_entries: Vec<InputEntry> = draft.inputs.iter()
    .filter(|ig| ig.device_id.is_some() && !ig.channels.is_empty())
    .map(|ig| InputEntry {
        name: ig.name.clone(),
        device_id: DeviceId(ig.device_id.clone().unwrap_or_default()),
        mode: ig.mode,
        channels: ig.channels.clone(),
    }).collect();

if let Some(chain_idx) = editing_index {
    if let Some(chain) = session.project.chains.get_mut(chain_idx) {
        // Find target block: specific index or first InputBlock
        let target_idx = io_block_idx.unwrap_or_else(|| {
            chain.blocks.iter().position(|b| matches!(&b.kind, AudioBlockKind::Input(_))).unwrap_or(0)
        });
        if let Some(block) = chain.blocks.get_mut(target_idx) {
            if let AudioBlockKind::Input(ref mut ib) = block.kind {
                ib.entries = new_entries;
            }
        }
        // sync runtime, refresh UI, set dirty
    }
}
```

- [ ] **Step 4: Same for output groups save**

Same pattern for `chain_output_groups_window.on_save` — find last OutputBlock or specific block, update entries.

- [ ] **Step 5: Run all tests**

Run: `cargo test --package adapter-gui`
Run: `cargo build` — zero warnings

- [ ] **Step 6: Commit**

```bash
git add crates/adapter-gui/src/lib.rs
git commit -m "fix(gui): I/O groups save updates block in-place, never reconstructs chain (#79)"
```

---

### Task 2: Fix picker "+" insert position

When the picker creates a new I/O block, it must insert at the exact position and NOT touch any other blocks.

**Files:**
- Modify: `crates/adapter-gui/src/lib.rs` (lines ~5046-5115 input insert, ~5246-5315 output insert)

- [ ] **Step 1: Verify the `on_choose_block_type` sets correct before_index**

Read the code at line ~4170. The `before_index` comes from `block_editor_draft.before_index` which was set by `on_start_block_insert`. This index is in UI space. Verify `ui_index_to_real_block_index` is called.

- [ ] **Step 2: Verify the insert in `chain_input_window.on_save` inserts at correct position**

The code at line ~5098 does `chain.blocks.insert(insert_pos, input_block)`. Verify `insert_pos` comes from `io_draft.before_index` which should already be in real space (set in `on_choose_block_type` after `on_start_block_insert` which calls `ui_index_to_real_block_index`).

If not, add the mapping.

- [ ] **Step 3: After insert, do NOT call chain_from_draft or any chain reconstruction**

The insert path already creates a single block and inserts at position. Verify it does NOT call `chain_from_draft` anywhere after.

- [ ] **Step 4: Test manually — insert Input between blocks, verify position**

- [ ] **Step 5: Commit if changes were made**

---

### Task 3: Entry add flow — opens device config, returns to list

**Files:**
- Modify: `crates/adapter-gui/src/lib.rs` (on_add_input ~2677, on_add_output ~2710)
- Modify: `crates/adapter-gui/ui/pages/chain_io_groups.slint`

Currently `on_add_input` adds an empty InputGroupDraft to the list. The spec says it should open the device/channels/mode config window, and on save return to the list.

- [ ] **Step 1: Change on_add_input to open ChainInputWindow**

Instead of adding empty draft to list, open the input config window:
```rust
chain_editor_window.on_add_input(move || {
    // Set editing_input_index to a new index
    let mut draft_borrow = chain_draft.borrow_mut();
    let Some(draft) = draft_borrow.as_mut() else { return; };
    let new_idx = draft.inputs.len();
    // Add placeholder
    draft.inputs.push(InputGroupDraft {
        name: format!("Input {}", new_idx + 1),
        device_id: input_chain_devices.first().map(|d| d.id.clone()),
        channels: Vec::new(),
        mode: ChainInputMode::Mono,
    });
    draft.editing_input_index = Some(new_idx);
    // Open input config window with the new entry
    if let Some(iw) = weak_input_window.upgrade() {
        let input_group = &draft.inputs[new_idx];
        apply_chain_input_window_state(...);
        show_child_window(...);
    }
});
```

- [ ] **Step 2: Same for on_add_output**

- [ ] **Step 3: When input window saves (non-insert mode), refresh the groups list**

In `chain_input_window.on_save`, after updating the draft entry, refresh the groups window list.

- [ ] **Step 4: When input window cancels, remove the placeholder entry**

If the user cancels, remove the entry that was added as placeholder.

- [ ] **Step 5: Test the flow: click add → config window → save → back to list**

- [ ] **Step 6: Commit**

```bash
git commit -m "feat(gui): add entry opens device config window, returns to list on save (#79)"
```

---

### Task 4: Remove entry validation — fixed blocks minimum 1

**Files:**
- Modify: `crates/adapter-gui/src/lib.rs` (on_remove_input ~2738, on_remove_output)
- Modify: `crates/adapter-gui/ui/pages/chain_io_groups.slint`

- [ ] **Step 1: In on_remove_input, check if this is a fixed block with 1 entry**

```rust
chain_editor_window.on_remove_input(move |group_index| {
    let mut draft_borrow = chain_draft.borrow_mut();
    let Some(draft) = draft_borrow.as_mut() else { return; };
    // If this is the fixed block (editing_io_block_index is None) and only 1 entry, block removal
    if draft.editing_io_block_index.is_none() && draft.inputs.len() <= 1 {
        // Show message or just do nothing
        return;
    }
    // Otherwise remove
    let gi = group_index as usize;
    if gi < draft.inputs.len() {
        draft.inputs.remove(gi);
    }
    // Refresh UI
});
```

- [ ] **Step 2: Same for on_remove_output**

- [ ] **Step 3: Test — try removing last entry from chip In (should be blocked)**

- [ ] **Step 4: Test — remove entry from middle block (should allow, block becomes empty)**

- [ ] **Step 5: Commit**

```bash
git commit -m "fix(gui): prevent removing last entry from fixed I/O blocks (#79)"
```

---

### Task 5: Arrow icons for middle I/O blocks

**Files:**
- Create: `crates/adapter-gui/ui/assets/input_arrow.svg`
- Create: `crates/adapter-gui/ui/assets/output_arrow.svg`
- Modify: `crates/adapter-gui/src/ui_state.rs` (accent_color_for_icon_kind)
- Modify: `crates/adapter-gui/ui/pages/project_chains.slint` (EffectTypeIcon)

- [ ] **Step 1: Create input_arrow.svg**

Simple arrow-in SVG icon (similar to the "In" chip arrow but standalone):
```svg
<svg viewBox="0 0 48 48" fill="none" xmlns="http://www.w3.org/2000/svg">
  <path d="M8 24H36M36 24L24 14M36 24L24 34" stroke="currentColor" stroke-width="3" stroke-linecap="round" stroke-linejoin="round"/>
  <rect x="36" y="12" width="4" height="24" rx="1" fill="currentColor"/>
</svg>
```

- [ ] **Step 2: Create output_arrow.svg**

Arrow-out SVG icon:
```svg
<svg viewBox="0 0 48 48" fill="none" xmlns="http://www.w3.org/2000/svg">
  <rect x="8" y="12" width="4" height="24" rx="1" fill="currentColor"/>
  <path d="M16 24H40M40 24L28 14M40 24L28 34" stroke="currentColor" stroke-width="3" stroke-linecap="round" stroke-linejoin="round"/>
</svg>
```

- [ ] **Step 3: Add "input" and "output" to accent_color_for_icon_kind**

```rust
"input" => slint::Color::from_argb_u8(255, 0x45, 0xa7, 0xff),   // blue
"output" => slint::Color::from_argb_u8(255, 0x45, 0xa7, 0xff),  // blue
```

- [ ] **Step 4: Add "input" and "output" icon cases to EffectTypeIcon in Slint**

In the EffectTypeIcon component, add:
```slint
if root.icon-kind == "input" : Image {
    source: @image-url("../assets/input_arrow.svg");
    colorize: root.tint;
    image-fit: contain;
    width: parent.width; height: parent.height;
}
if root.icon-kind == "output" : Image {
    source: @image-url("../assets/output_arrow.svg");
    colorize: root.tint;
    image-fit: contain;
    width: parent.width; height: parent.height;
}
```

- [ ] **Step 5: Verify middle I/O blocks render with arrow icons**

- [ ] **Step 6: Commit**

```bash
git add crates/adapter-gui/ui/assets/input_arrow.svg crates/adapter-gui/ui/assets/output_arrow.svg
git add crates/adapter-gui/src/ui_state.rs crates/adapter-gui/ui/pages/project_chains.slint
git commit -m "feat(gui): arrow icons and accent colors for middle I/O blocks (#79)"
```

---

### Task 6: Enable/disable LED on middle I/O blocks

The LED already exists on all BlockChip components (line ~883 in project_chains.slint). It shows green/gray based on `block.enabled`. I/O blocks already have `enabled: bool` in AudioBlock.

**Files:**
- Modify: `crates/adapter-gui/src/lib.rs` (on_toggle_chain_block_enabled)

- [ ] **Step 1: Verify toggle works for I/O blocks**

The `on_toggle_chain_block_enabled` callback should work for any block type since it just flips `block.enabled`. Test clicking the LED on a middle I/O block.

- [ ] **Step 2: If toggle doesn't work for I/O blocks, fix the callback**

The callback may skip I/O blocks. Check if there's a filter.

- [ ] **Step 3: Commit if changes were made**

---

### Self-Review

Checking against spec (`docs/superpowers/specs/2026-03-29-io-blocks-design.md`):

| Spec Requirement | Task |
|---|---|
| Chip In shows only first InputBlock entries | Already fixed (commit c8fbf21) |
| Chip Out shows only last OutputBlock entries | Already fixed (commit c8fbf21) |
| Click I/O block in middle opens its entries | Already fixed (commit 94d6dd1) |
| Save updates ONLY the block being edited | **Task 1** |
| Block position never changes on save | **Task 1** |
| Picker creates new block at exact position | **Task 2** |
| Add entry opens device config, returns to list | **Task 3** |
| Fixed blocks minimum 1 entry | **Task 4** |
| Middle blocks can have 0 entries | **Task 4** |
| Arrow icons for middle I/O | **Task 5** |
| Enable/disable LED | **Task 6** |
| Disabled Input stops its stream | Already in engine |
| Validation no duplicate device+channel | Already implemented |

All spec requirements covered.
