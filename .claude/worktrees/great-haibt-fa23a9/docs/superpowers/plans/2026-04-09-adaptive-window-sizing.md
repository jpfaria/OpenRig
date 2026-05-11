# Adaptive Window Sizing Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Make OpenRig windows fit on ~1300×700px screens (common Windows notebooks) without overflowing on 4K displays.

**Architecture:** Two changes: (1) reduce hardcoded `min-*` and `preferred-*` sizes in Slint so the user can resize small, and (2) in Rust, after creating the main `AppWindow`, read the screen size and set the initial window size using the formula `clamp(screen × factor, min, max)`.

**Tech Stack:** Rust, Slint 1.14, `slint::LogicalSize`, `slint::Window::screen_size()`, `slint::Window::scale_factor()`.

---

## File Map

| File | Change |
|------|--------|
| `crates/adapter-gui/ui/app-window.slint` | Reduce `min-*` and `preferred-*` on all Window components |
| `crates/adapter-gui/src/lib.rs` | Add adaptive sizing after `AppWindow::new()` (line ~463) |

---

### Task 1: Reduce hardcoded window sizes in app-window.slint

**Files:**
- Modify: `crates/adapter-gui/ui/app-window.slint`

Context: There are 8 Window components with hardcoded sizes. The main/settings/chain-editor windows have `preferred 1160×760, min 980×680`. Secondary windows have `preferred 860×720, min 720×620`. `ChainInsertWindow` has `preferred 860×820, min 720×720`. `CompactChainViewWindow` has `preferred 1100×500, min 900×300`. `AppWindow` (launcher) has `preferred 1160×760, min 980×680`.

There is no automated way to test Slint window sizes — verify by building and running. The test for this task is: `cargo build -p adapter-gui` passes with zero warnings.

- [ ] **Step 1: Update ProjectSettingsWindow (line 27-30)**

Replace:
```slint
    min-width: 980px;
    min-height: 680px;
    preferred-width: 1160px;
    preferred-height: 760px;
```
With:
```slint
    min-width: 860px;
    min-height: 540px;
    preferred-width: 1100px;
    preferred-height: 640px;
```

- [ ] **Step 2: Update ChainEditorWindow (line 73-76)**

Replace:
```slint
    min-width: 980px;
    min-height: 680px;
    preferred-width: 1160px;
    preferred-height: 760px;
```
With:
```slint
    min-width: 860px;
    min-height: 540px;
    preferred-width: 1100px;
    preferred-height: 640px;
```

- [ ] **Step 3: Update ChainInputWindow (line 118-121)**

Replace:
```slint
    min-width: 720px;
    min-height: 620px;
    preferred-width: 860px;
    preferred-height: 720px;
```
With:
```slint
    min-width: 680px;
    min-height: 500px;
    preferred-width: 820px;
    preferred-height: 620px;
```

- [ ] **Step 4: Update ChainOutputWindow (line 158-161)**

Replace:
```slint
    min-width: 720px;
    min-height: 620px;
    preferred-width: 860px;
    preferred-height: 720px;
```
With:
```slint
    min-width: 680px;
    min-height: 500px;
    preferred-width: 820px;
    preferred-height: 620px;
```

- [ ] **Step 5: Update ChainInputGroupsWindow (line 199-202)**

Replace:
```slint
    min-width: 720px;
    min-height: 620px;
    preferred-width: 860px;
    preferred-height: 720px;
```
With:
```slint
    min-width: 680px;
    min-height: 500px;
    preferred-width: 820px;
    preferred-height: 620px;
```

- [ ] **Step 6: Update ChainOutputGroupsWindow (line 240-243)**

Replace:
```slint
    min-width: 720px;
    min-height: 620px;
    preferred-width: 860px;
    preferred-height: 720px;
```
With:
```slint
    min-width: 680px;
    min-height: 500px;
    preferred-width: 820px;
    preferred-height: 620px;
```

- [ ] **Step 7: Update ChainInsertWindow (line 296-299)**

Replace:
```slint
    min-width: 720px;
    min-height: 720px;
    preferred-width: 860px;
    preferred-height: 820px;
```
With:
```slint
    min-width: 680px;
    min-height: 580px;
    preferred-width: 820px;
    preferred-height: 720px;
```

- [ ] **Step 8: Update BlockEditorWindow fallback height (line 402, 406)**

The `BlockEditorWindow` uses a dynamic `min-height` based on `use-panel-editor`. The fallback value of `760px` (when not using panel editor) is too tall. Replace `760px` with `640px` in both lines:

```slint
    min-height: root.use-panel-editor ? eq-extra-height : 640px;
    ...
    preferred-height: root.use-panel-editor ? eq-extra-height : 640px;
```

- [ ] **Step 9: Update CompactChainViewWindow (line 489-492)**

Replace:
```slint
    min-width: 900px;
    min-height: 300px;
    preferred-width: 1100px;
    preferred-height: 500px;
```
With:
```slint
    min-width: 800px;
    min-height: 260px;
    preferred-width: 1000px;
    preferred-height: 460px;
```

- [ ] **Step 10: Update AppWindow (line 656-659)**

Replace:
```slint
    min-width: 980px;
    min-height: 680px;
    preferred-width: 1160px;
    preferred-height: 760px;
```
With:
```slint
    min-width: 860px;
    min-height: 540px;
    preferred-width: 1100px;
    preferred-height: 640px;
```

- [ ] **Step 11: Build and verify zero warnings**

```bash
cargo build -p adapter-gui 2>&1 | grep -E "^error|^warning"
```
Expected: no output (zero errors, zero warnings).

- [ ] **Step 12: Commit**

```bash
git add crates/adapter-gui/ui/app-window.slint
git commit -m "feat(ui): reduce window min/preferred sizes for small screens"
```

---

### Task 2: Adaptive sizing from Rust at startup

**Files:**
- Modify: `crates/adapter-gui/src/lib.rs` (after line 463)

Context: After `AppWindow::new()` at line 463, we query the screen size in logical pixels and set the window size using the formula: `w = clamp(screen_w × 0.85, 860, 1400)`, `h = clamp(screen_h × 0.80, 540, 860)`.

`slint::Window::screen_size()` returns `PhysicalSize { width: u32, height: u32 }` in physical pixels. Divide by `scale_factor()` to get logical pixels.

There are no unit tests for this — the test is: build succeeds and on a 1300×700 screen the window opens fully visible.

- [ ] **Step 1: Add adaptive sizing after AppWindow::new()**

In `crates/adapter-gui/src/lib.rs`, find the line:
```rust
    let window = AppWindow::new().map_err(|error| anyhow!(error.to_string()))?;
```
(line ~463)

Add immediately after it:
```rust
    {
        let screen = window.window().screen_size();
        let scale = window.window().scale_factor() as f32;
        let screen_w = screen.width as f32 / scale;
        let screen_h = screen.height as f32 / scale;
        let w = (screen_w * 0.85).clamp(860.0, 1400.0);
        let h = (screen_h * 0.80).clamp(540.0, 860.0);
        window.window().set_size(slint::LogicalSize { width: w, height: h });
    }
```

- [ ] **Step 2: Build and verify zero warnings**

```bash
cargo build -p adapter-gui 2>&1 | grep -E "^error|^warning"
```
Expected: no output.

- [ ] **Step 3: Commit**

```bash
git add crates/adapter-gui/src/lib.rs
git commit -m "feat(ui): adaptive window sizing based on screen resolution"
```

---

### Task 3: Update docs and push

**Files:**
- Modify: `CLAUDE.md` (Telas principais section — note adaptive sizing)
- Push branch

- [ ] **Step 1: Update CLAUDE.md**

In `CLAUDE.md`, under "Telas principais", add a note after the screens list:

```markdown
**Tamanhos de janela:** Adaptativos ao tamanho da tela. Fórmula: `w = clamp(screen_w × 0.85, 860, 1400)`, `h = clamp(screen_h × 0.80, 540, 860)`. Mínimos definidos no Slint garantem que o usuário pode redimensionar livremente.
```

- [ ] **Step 2: Commit docs**

```bash
git add CLAUDE.md
git commit -m "docs: document adaptive window sizing behavior"
```

- [ ] **Step 3: Push branch**

```bash
git push -u origin feature/issue-240
```

---

## Verification

After all tasks, run the app and verify:
```bash
cargo run
```
- Window opens at a size that fits the current screen
- Window can be resized smaller than the old 680px min-height
- On macOS (dev machine), window is not absurdly large
