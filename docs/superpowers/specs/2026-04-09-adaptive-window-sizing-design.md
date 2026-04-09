# Adaptive Window Sizing Design

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Adjust window sizes so OpenRig fits on low-resolution screens (~1300×700px, common on Windows notebooks) while preventing oversized windows on 4K displays.

---

## Problem

Main window has `min-height: 680px` and `preferred-height: 760px`. On a ~1300×700px screen (with OS chrome: taskbar ~40px + title bar ~30px = ~70px), the usable area is ~630px — the window opens cropped and cannot be resized smaller.

---

## Solution

Two combined changes:

### 1. Reduce hardcoded Slint sizes

New values sized for 1300×700px screens (630px usable height):

| Window component | preferred (old) | preferred (new) | min (old) | min (new) |
|-----------------|-----------------|-----------------|-----------|-----------|
| Main / Launcher | 1160×760 | 1100×640 | 980×680 | 860×540 |
| Setup / Settings / Chain Editor | 860×720 | 820×620 | 720×620 | 680×500 |
| Block editor popup | 860×820 | 820×720 | 720×720 | 680×580 |
| Plugin info | 1100×500 | 1000×460 | 900×300 | 800×260 |

### 2. Rust adaptive sizing at startup

After creating the main window in `lib.rs`, calculate the ideal size based on the actual screen:

```
width  = clamp(screen_width_logical  × 0.85, 860.0, 1400.0)
height = clamp(screen_height_logical × 0.80, 540.0, 860.0)
```

Apply with `window.window().set_size(slint::LogicalSize { width, height })`.

Result across resolutions:

| Screen | Raw calc | Final size |
|--------|----------|------------|
| 1300×700 | 1105×560 | 1105×560 ✓ |
| 1920×1080 | 1632×864 | 1400×860 (capped) ✓ |
| 2560×1440 | 2176×1152 | 1400×860 (capped) ✓ |
| 4K 3840×2160 | 3264×1728 | 1400×860 (capped) ✓ |

---

## Screen size detection (Slint 1.14)

From Rust, after `AppWindow::new()`, before `show()`:

```rust
let screen = window.window().screen_size();          // PhysicalSize
let scale  = window.window().scale_factor() as f32;
let screen_w = screen.width  as f32 / scale;
let screen_h = screen.height as f32 / scale;

let w = (screen_w * 0.85).clamp(860.0, 1400.0);
let h = (screen_h * 0.80).clamp(540.0,  860.0);

window.window().set_size(slint::LogicalSize { width: w, height: h });
```

---

## Files

| Action | Path |
|--------|------|
| Modify | `crates/adapter-gui/ui/app-window.slint` |
| Modify | `crates/adapter-gui/src/lib.rs` |

---

## Out of scope

- Touch/mobile layouts
- Per-window adaptive sizing (only main window gets Rust adaptive sizing; secondary windows use the new hardcoded defaults)
