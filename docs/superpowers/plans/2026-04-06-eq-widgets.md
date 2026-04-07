# EQ Widgets Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Complete the EQ widget UI for parametric and graphic EQ blocks — interactive draggable controls with frequency response curve.

**Architecture:** The block-core/block-filter/adapter-gui Rust layers are already complete. The remaining work is: (1) fix a build error in touch_main.slint, (2) create two new Slint components (CurveEditorControl and MultiSliderControl), (3) integrate them in block_panel_editor.slint, and (4) add frequency response curve computation in Rust pushed to Slint as SVG path strings.

**Tech Stack:** Slint (UI), Rust (curve computation), existing `update-block-parameter-number(path, value)` callback for parameter updates.

**Working directory:** `/Users/joao.faria/Projetos/github.com/jpfaria/OpenRig/.solvers/issue-156`

---

## What is already done (do NOT redo)

- `block-core`: `BiquadFilter`, `ParameterWidget::MultiSlider`, `ParameterWidget::CurveEditor { role }`, `CurveEditorRole::{X,Y,Width}`, helper functions
- `block-filter`: Three Band EQ redesigned with biquad DSP; all 5 EQ models use `curve_editor_parameter` / `multi_slider_parameter`
- `adapter-gui/src/lib.rs`: `build_curve_editor_points()`, `build_multi_slider_points()`, `BAND_COLORS`, wiring of `multi_slider_points` and `curve_editor_points` model vecs
- `models.slint`: `MultiSliderPoint`, `CurveEditorPoint` structs
- `app-window.slint`: `multi-slider-points` and `curve-editor-points` properties propagated through `AppWindow`, `BlockEditorWindow`, `BlockPanelEditor`

## File Map

| File | Action | What it does |
|------|--------|--------------|
| `crates/adapter-gui/ui/touch_main.slint` | Modify | Add missing `multi-slider-points`, `curve-editor-points` properties and wiring |
| `crates/adapter-gui/ui/pages/curve_editor_control.slint` | **Create** | Interactive parametric EQ widget — draggable points + frequency response curve |
| `crates/adapter-gui/ui/pages/multi_slider_control.slint` | **Create** | Graphic EQ widget — 31 vertical sliders with response curve |
| `crates/adapter-gui/ui/pages/block_panel_editor.slint` | Modify | Render `CurveEditorControl` or `MultiSliderControl` when appropriate points exist |
| `crates/adapter-gui/ui/pages/pages.slint` | Modify | Export new components |
| `crates/adapter-gui/src/lib.rs` | Modify | Add `compute_eq_curves()`, push `eq-total-curve` + `eq-band-curves` to Slint on parameter changes |
| `crates/adapter-gui/ui/app-window.slint` | Modify | Propagate `eq-total-curve`, `eq-band-curves` properties |
| `crates/adapter-gui/ui/touch_main.slint` | Modify | Propagate `eq-total-curve`, `eq-band-curves` properties |

---

## Task 1: Fix build — add missing properties to touch_main.slint

**Files:**
- Modify: `crates/adapter-gui/ui/touch_main.slint`

The build fails because `app-window.slint` passes `multi-slider-points` and `curve-editor-points` to `TouchMain`, but `TouchMain` doesn't declare them. Also need to pass them through to `ProjectChainsPage`.

- [ ] **Step 1: Add properties to TouchMain**

In `touch_main.slint`, after line 48 (`in property <[BlockParameterItem]> block-parameter-items;`):

```slint
    in property <[MultiSliderPoint]> multi-slider-points;
    in property <[CurveEditorPoint]> curve-editor-points;
```

- [ ] **Step 2: Wire properties to inner component**

In the usage of `ProjectChainsPage` inside `TouchMain` (around line 163 after `block-parameter-items: root.block-parameter-items;`):

```slint
            multi-slider-points: root.multi-slider-points;
            curve-editor-points: root.curve-editor-points;
```

- [ ] **Step 3: Verify build passes**

```bash
cargo build -p adapter-gui 2>&1 | tail -5
```

Expected: `Finished` with no errors.

Note: `ProjectChainsPage` may also need these properties added — follow the same pattern: declare as `in property` in the component and pass through to wherever `BlockPanelEditor` is used. Check all intermediate components in the chain.

- [ ] **Step 4: Commit**

```bash
git add crates/adapter-gui/ui/touch_main.slint
git commit -m "fix: propagate multi-slider-points and curve-editor-points through TouchMain"
```

---

## Task 2: Create curve_editor_control.slint

**Files:**
- Create: `crates/adapter-gui/ui/pages/curve_editor_control.slint`

This component renders a 2D EQ canvas with colored draggable control points (one per `CurveEditorPoint`). It receives a list of `CurveEditorPoint` and uses the existing `update-block-parameter-number` callback to update parameters on drag.

Coordinate system:
- X axis: frequency 20Hz–20kHz, logarithmic scale, left→right
- Y axis: gain in dB, min/max per band, top=max, bottom=min. For a universal display, use -18dB to +18dB canvas with band-specific clamping.
- Canvas size: fills parent width, fixed 200px height.

Helper functions (pure, in the component):
- `freq-to-norm(freq: float, fmin: float, fmax: float) -> float` — log normalized x in [0,1]
- `norm-to-freq(norm: float, fmin: float, fmax: float) -> float` — denormalize x
- `gain-to-norm(gain: float, gmin: float, gmax: float) -> float` — linear normalized y in [0,1], inverted (top=max)
- `norm-to-gain(norm: float, gmin: float, gmax: float) -> float`

- [ ] **Step 1: Create file with component skeleton**

```slint
import { CurveEditorPoint } from "../models.slint";

export component CurveEditorControl inherits Rectangle {
    in property <[CurveEditorPoint]> points;
    in property <string> total-curve;          // SVG path in 0..1000 × 0..200 space
    in property <[string]> band-curves;        // per-band SVG paths

    callback update-parameter(string, float);  // path, value — wired to update-block-parameter-number

    private property <float> CANVAS-W: 1000.0;
    private property <float> CANVAS-H: 200.0;
    private property <float> GAIN-CANVAS-MIN: -18.0;
    private property <float> GAIN-CANVAS-MAX: 18.0;

    height: 200px;
    background: #06060e;
    border-radius: 6px;
    clip: true;

    // 0 dB line
    Rectangle {
        x: 0; y: parent.height / 2;
        width: parent.width; height: 1px;
        background: #ffffff30;
    }
    // +6 dB line
    Rectangle {
        x: 0;
        y: parent.height / 2 - parent.height * (6.0 / 36.0);
        width: parent.width; height: 1px;
        background: #ffffff10;
    }
    // -6 dB line
    Rectangle {
        x: 0;
        y: parent.height / 2 + parent.height * (6.0 / 36.0);
        width: parent.width; height: 1px;
        background: #ffffff10;
    }
    // +12 dB line
    Rectangle {
        x: 0;
        y: parent.height / 2 - parent.height * (12.0 / 36.0);
        width: parent.width; height: 1px;
        background: #ffffff08;
    }
    // -12 dB line
    Rectangle {
        x: 0;
        y: parent.height / 2 + parent.height * (12.0 / 36.0);
        width: parent.width; height: 1px;
        background: #ffffff08;
    }
    // 100Hz vertical
    Rectangle {
        x: freq-to-x(100.0) * parent.width; y: 0;
        width: 1px; height: parent.height;
        background: #ffffff08;
    }
    // 1kHz vertical
    Rectangle {
        x: freq-to-x(1000.0) * parent.width; y: 0;
        width: 1px; height: parent.height;
        background: #ffffff10;
    }
    // 10kHz vertical
    Rectangle {
        x: freq-to-x(10000.0) * parent.width; y: 0;
        width: 1px; height: parent.height;
        background: #ffffff08;
    }

    // Axis labels
    Text { x: freq-to-x(100.0) * parent.width + 2px; y: parent.height - 14px; text: "100"; color: #ffffff30; font-size: 9px; }
    Text { x: freq-to-x(1000.0) * parent.width + 2px; y: parent.height - 14px; text: "1k"; color: #ffffff40; font-size: 9px; }
    Text { x: freq-to-x(10000.0) * parent.width + 2px; y: parent.height - 14px; text: "10k"; color: #ffffff30; font-size: 9px; }

    // Band curves (semi-transparent, one per point)
    for curve[i] in root.band-curves : Path {
        width: parent.width; height: parent.height;
        viewbox-x: 0; viewbox-y: 0;
        viewbox-width: root.CANVAS-W; viewbox-height: root.CANVAS-H;
        commands: curve;
        fill: transparent;
        stroke: root.points[i].band-color;
        stroke-width: 1.5px;
        opacity: 0.4;
    }

    // Total response curve (white)
    Path {
        visible: root.total-curve != "";
        width: parent.width; height: parent.height;
        viewbox-x: 0; viewbox-y: 0;
        viewbox-width: root.CANVAS-W; viewbox-height: root.CANVAS-H;
        commands: root.total-curve;
        fill: transparent;
        stroke: #ffffffc0;
        stroke-width: 2px;
    }

    // Control points
    for point[i] in root.points : Rectangle {
        property <float> start-x-val: 0.0;
        property <float> start-y-val: 0.0;
        property <float> start-w-val: 0.0;

        private property <float> cx: point.has-x
            ? freq-to-x(point.x-value)
            : 0.5;
        private property <float> cy: gain-to-y(point.y-value, point.y-min, point.y-max);

        x: self.cx * parent.width - 8px;
        y: self.cy * parent.height - 8px;
        width: 16px; height: 16px;
        border-radius: 8px;
        background: point.band-color;
        border-width: 1.5px;
        border-color: #ffffff60;

        ta := TouchArea {
            mouse-cursor: move;
            pressed => {
                parent.start-x-val = point.x-value;
                parent.start-y-val = point.y-value;
                parent.start-w-val = point.width-value;
            }
            moved => {
                if point.has-x {
                    let dx = (self.mouse-x - self.pressed-x) / parent.parent.width;
                    let new-norm-x = Math.clamp(freq-to-x(parent.start-x-val) + dx, 0.0, 1.0);
                    let new-freq = Math.clamp(
                        norm-to-freq(new-norm-x),
                        point.x-min, point.x-max);
                    root.update-parameter(point.x-path, new-freq);
                }
                let dy = (self.mouse-y - self.pressed-y) / parent.parent.height;
                let new-norm-y = Math.clamp(gain-to-y(parent.start-y-val, point.y-min, point.y-max) + dy, 0.0, 1.0);
                let new-gain = Math.clamp(
                    norm-to-gain(new-norm-y, point.y-min, point.y-max),
                    point.y-min, point.y-max);
                root.update-parameter(point.y-path, new-gain);
            }
        }

        // Q wing handle — left side (only for bands with width/Q)
        if point.has-width : Rectangle {
            property <float> q-norm: Math.clamp(point.width-value / (point.width-max - point.width-min), 0.0, 1.0);
            // handle is to the left at distance proportional to 1/width (wider Q = closer handles)
            property <float> handle-offset-norm: 0.08 / Math.max(0.1, point.width-value);
            property <float> handle-norm-x: Math.clamp(freq-to-x(point.x-value) - self.handle-offset-norm, 0.02, 0.98);

            x: self.handle-norm-x * parent.parent.width - 5px - parent.x;
            y: 3px;
            width: 10px; height: 10px;
            border-radius: 5px;
            background: point.band-color;
            opacity: 0.7;

            TouchArea {
                mouse-cursor: ew-resize;
                moved => {
                    let dx = (self.mouse-x - self.pressed-x) / parent.parent.parent.width;
                    // moving handle left = wider (lower width value)
                    let new-norm-x = Math.clamp(parent.handle-norm-x - dx, 0.02, 0.98);
                    let new-offset = freq-to-x(point.x-value) - new-norm-x;
                    let new-bw = Math.clamp(0.08 / Math.max(0.001, new-offset),
                        point.width-min, point.width-max);
                    root.update-parameter(point.width-path, new-bw);
                }
            }
        }

        // Q wing handle — right side
        if point.has-width : Rectangle {
            property <float> handle-offset-norm: 0.08 / Math.max(0.1, point.width-value);
            property <float> handle-norm-x: Math.clamp(freq-to-x(point.x-value) + self.handle-offset-norm, 0.02, 0.98);

            x: self.handle-norm-x * parent.parent.width - 5px - parent.x;
            y: 3px;
            width: 10px; height: 10px;
            border-radius: 5px;
            background: point.band-color;
            opacity: 0.7;

            TouchArea {
                mouse-cursor: ew-resize;
                moved => {
                    let dx = (self.mouse-x - self.pressed-x) / parent.parent.parent.width;
                    let new-norm-x = Math.clamp(parent.handle-norm-x + dx, 0.02, 0.98);
                    let new-offset = new-norm-x - freq-to-x(point.x-value);
                    let new-bw = Math.clamp(0.08 / Math.max(0.001, new-offset),
                        point.width-min, point.width-max);
                    root.update-parameter(point.width-path, new-bw);
                }
            }
        }
    }

    // Helper functions
    pure function freq-to-x(freq: float) -> float {
        // 20Hz-20kHz log scale → 0..1
        Math.log(Math.max(20.0, freq) / 20.0) / Math.log(1000.0)
    }

    pure function norm-to-freq(norm: float) -> float {
        20.0 * Math.pow(1000.0, norm)
    }

    pure function gain-to-y(gain: float, gmin: float, gmax: float) -> float {
        // inverted: top = max gain, bottom = min gain
        1.0 - (gain - gmin) / (gmax - gmin)
    }

    pure function norm-to-gain(norm: float, gmin: float, gmax: float) -> float {
        gmin + (1.0 - norm) * (gmax - gmin)
    }
}
```

- [ ] **Step 2: Verify it compiles (as part of adapter-gui build in Task 4)**

No standalone compile step — will be verified in Task 4 after integration.

---

## Task 3: Create multi_slider_control.slint

**Files:**
- Create: `crates/adapter-gui/ui/pages/multi_slider_control.slint`

Renders 31 (or N) vertical sliders side by side. Each `MultiSliderPoint` gets one slider. The slider thumb is a horizontal line that the user drags vertically.

- [ ] **Step 1: Create file**

```slint
import { MultiSliderPoint } from "../models.slint";

export component MultiSliderControl inherits Rectangle {
    in property <[MultiSliderPoint]> points;

    callback update-parameter(string, float);

    height: 180px;
    background: #06060e;
    border-radius: 6px;
    clip: true;

    // 0 dB center line
    Rectangle {
        x: 0; y: parent.height / 2;
        width: parent.width; height: 1px;
        background: #ffffff30;
    }

    // Sliders
    for point[i] in root.points : Rectangle {
        property <float> norm: (point.value - point.min-val) / (point.max-val - point.min-val);

        private property <length> slider-w: Math.max(4px, (parent.width - 2px) / Math.max(1, root.points.length) - 1px);
        x: i * (self.slider-w + 1px);
        y: 0;
        width: self.slider-w;
        height: parent.height;

        // Track
        Rectangle {
            x: parent.width / 2 - 1px;
            y: 4px;
            width: 2px;
            height: parent.height - 8px;
            background: #ffffff15;
        }

        // Fill (from center to thumb)
        Rectangle {
            private property <float> center-norm: (0.0 - point.min-val) / (point.max-val - point.min-val);
            private property <length> center-y: (1.0 - parent.norm) * (parent.height - 8px) + 4px;
            private property <length> zero-y: (1.0 - self.center-norm) * (parent.height - 8px) + 4px;

            x: parent.width / 2 - 2px;
            y: Math.min(self.center-y, self.zero-y);
            width: 4px;
            height: Math.abs(self.center-y - self.zero-y);
            background: #4488ff80;
        }

        // Thumb
        Rectangle {
            x: 2px;
            y: (1.0 - parent.norm) * (parent.height - 8px) + 4px - 2px;
            width: parent.width - 4px;
            height: 4px;
            border-radius: 2px;
            background: #88aaff;

            TouchArea {
                mouse-cursor: ns-resize;
                moved => {
                    let dy = (self.mouse-y - self.pressed-y) / parent.parent.height;
                    let new-norm = Math.clamp(parent.parent.norm - dy, 0.0, 1.0);
                    let new-val = Math.clamp(
                        point.min-val + new-norm * (point.max-val - point.min-val),
                        point.min-val, point.max-val);
                    root.update-parameter(point.path, new-val);
                }
            }
        }
    }
}
```

---

## Task 4: Export new components and integrate in block_panel_editor.slint

**Files:**
- Modify: `crates/adapter-gui/ui/pages/pages.slint`
- Modify: `crates/adapter-gui/ui/pages/block_panel_editor.slint`

- [ ] **Step 1: Export from pages.slint**

Find `pages.slint` and add:

```slint
import { CurveEditorControl } from "curve_editor_control.slint";
import { MultiSliderControl } from "multi_slider_control.slint";
export { CurveEditorControl, MultiSliderControl }
```

(Or add exports alongside existing exports in that file — follow the existing pattern.)

- [ ] **Step 2: Import in block_panel_editor.slint**

At the top of `block_panel_editor.slint`, add import:

```slint
import { CurveEditorControl, MultiSliderControl } from "pages.slint";
```

- [ ] **Step 3: Add eq curve properties to BlockPanelEditor**

After the existing `in property <[CurveEditorPoint]> curve-editor-points;` line, add:

```slint
    in property <string> eq-total-curve;
    in property <[string]> eq-band-curves;
```

- [ ] **Step 4: Render EQ widgets in the parameter area**

In `block_panel_editor.slint`, find the area where parameters are rendered (around the section `// ── All parameters ...`). Add the EQ widgets as alternatives to the standard parameter list.

The existing pattern shows parameters when `block-knob-overlays.length == 0`. Add:

```slint
        // ── Curve Editor (parametric EQ) ──
        if root.curve-editor-points.length > 0 : CurveEditorControl {
            x: 8px;
            y: /* position below block header — find the appropriate y offset */;
            width: parent.width - 16px;
            points: root.curve-editor-points;
            total-curve: root.eq-total-curve;
            band-curves: root.eq-band-curves;
            update-parameter(path, value) => { root.update-block-parameter-number(path, value); }
        }

        // ── Multi Slider (graphic EQ) ──
        if root.multi-slider-points.length > 0 && root.curve-editor-points.length == 0 : MultiSliderControl {
            x: 8px;
            y: /* same y offset */;
            width: parent.width - 16px;
            points: root.multi-slider-points;
            update-parameter(path, value) => { root.update-block-parameter-number(path, value); }
        }
```

Find the exact `y` offset by looking at how other blocks (preamp panel image) are positioned in the file — search for `y: 132px` or similar.

The standard parameter rows should be hidden when EQ widgets are shown:
```slint
        // ── All parameters (HorizontalLayout) ──
        // Add to the existing `visible:` condition:
        visible: root.block-knob-overlays.length == 0
            && root.curve-editor-points.length == 0
            && root.multi-slider-points.length == 0;
```

- [ ] **Step 5: Build and fix any Slint compile errors**

```bash
cargo build -p adapter-gui 2>&1 | grep "error\|warning.*unused" | head -20
```

Fix any errors. Common issues:
- Wrong `y` offset — adjust to match surrounding UI
- `parent.parent` reference depth off by one — count nesting levels carefully

- [ ] **Step 6: Commit**

```bash
git add crates/adapter-gui/ui/pages/curve_editor_control.slint \
        crates/adapter-gui/ui/pages/multi_slider_control.slint \
        crates/adapter-gui/ui/pages/pages.slint \
        crates/adapter-gui/ui/pages/block_panel_editor.slint
git commit -m "feat: add CurveEditorControl and MultiSliderControl Slint widgets"
```

---

## Task 5: Propagate eq-total-curve and eq-band-curves through Slint hierarchy

**Files:**
- Modify: `crates/adapter-gui/ui/app-window.slint`
- Modify: `crates/adapter-gui/ui/touch_main.slint`
- Modify: `crates/adapter-gui/ui/pages/project_chains.slint` (if BlockPanelEditor is used there)

Same pattern as `multi-slider-points` / `curve-editor-points`. Add:

```slint
in property <string> eq-total-curve;
in property <[string]> eq-band-curves;
```

to each intermediate component and wire through to `BlockPanelEditor`.

- [ ] **Step 1: Grep for all components that pass curve-editor-points**

```bash
grep -rn "curve-editor-points" crates/adapter-gui/ui/ --include="*.slint"
```

Add `eq-total-curve` and `eq-band-curves` properties to every component in that list, using the same wiring pattern.

- [ ] **Step 2: Build and verify**

```bash
cargo build -p adapter-gui 2>&1 | tail -5
```

- [ ] **Step 3: Commit**

```bash
git add crates/adapter-gui/ui/
git commit -m "feat: propagate eq curve path properties through Slint hierarchy"
```

---

## Task 6: Implement frequency response curve computation in Rust

**Files:**
- Modify: `crates/adapter-gui/src/lib.rs`

Compute SVG path strings (one total + one per band) from `CurveEditorPoint` data using biquad magnitude response. Push them to Slint via `window.set_eq_total_curve(...)` and `window.set_eq_band_curves(...)`.

- [ ] **Step 1: Add curve computation function**

Add this function to `lib.rs` (before or after `build_curve_editor_points`):

```rust
/// Compute frequency response curves for a parametric EQ.
/// Returns (total_curve_svg, per_band_curve_svgs).
/// Coordinate space: x in 0..1000, y in 0..200 (0dB = 100, +18dB = 0, -18dB = 200).
fn compute_eq_curves(points: &[CurveEditorPoint]) -> (String, Vec<String>) {
    use std::f64::consts::PI;
    const FS: f64 = 44100.0;
    const N_POINTS: usize = 200;
    const FREQ_MIN: f64 = 20.0;
    const FREQ_MAX: f64 = 20000.0;
    const GAIN_MIN_DB: f64 = -18.0;
    const GAIN_MAX_DB: f64 = 18.0;
    const CANVAS_W: f64 = 1000.0;
    const CANVAS_H: f64 = 200.0;

    // Compute frequency array (log scale)
    let freqs: Vec<f64> = (0..N_POINTS)
        .map(|i| {
            let t = i as f64 / (N_POINTS - 1) as f64;
            FREQ_MIN * (FREQ_MAX / FREQ_MIN).powf(t)
        })
        .collect();

    // For each band, compute dB gain at each frequency
    let mut band_gains: Vec<Vec<f64>> = Vec::new();

    for point in points {
        let gain_db = point.y_value as f64;
        let freq_hz = if point.has_x { point.x_value as f64 } else { 1000.0 };
        // Determine filter type from group name (heuristic: Low/High Shelf vs Peak)
        let is_low_shelf = point.group.to_lowercase().contains("low") 
            && point.group.to_lowercase().contains("shelf");
        let is_high_shelf = point.group.to_lowercase().contains("high") 
            && point.group.to_lowercase().contains("shelf");
        // Q from width: if has_width, use it; else default Q=0.7071
        let q: f64 = if point.has_width {
            // width is bandwidth in octaves for TAP, or Q directly for others
            // Use 1/width as a rough Q estimate
            (1.0 / point.width_value.max(0.01) as f64).clamp(0.1, 10.0)
        } else {
            0.7071 // Butterworth (maximally flat)
        };

        let band_db: Vec<f64> = freqs.iter().map(|&f| {
            let w0 = 2.0 * PI * freq_hz / FS;
            let cos_w0 = w0.cos();
            let sin_w0 = w0.sin();
            let a = 10.0_f64.powf(gain_db / 40.0);
            let alpha = sin_w0 / (2.0 * q);

            let (b, a_coeffs) = if is_low_shelf {
                let two_sqrt_a_alpha = 2.0 * a.sqrt() * alpha;
                let b = [
                    a * ((a + 1.0) - (a - 1.0) * cos_w0 + two_sqrt_a_alpha),
                    2.0 * a * ((a - 1.0) - (a + 1.0) * cos_w0),
                    a * ((a + 1.0) - (a - 1.0) * cos_w0 - two_sqrt_a_alpha),
                ];
                let a_c = [
                    (a + 1.0) + (a - 1.0) * cos_w0 + two_sqrt_a_alpha,
                    -2.0 * ((a - 1.0) + (a + 1.0) * cos_w0),
                    (a + 1.0) + (a - 1.0) * cos_w0 - two_sqrt_a_alpha,
                ];
                (b, a_c)
            } else if is_high_shelf {
                let two_sqrt_a_alpha = 2.0 * a.sqrt() * alpha;
                let b = [
                    a * ((a + 1.0) + (a - 1.0) * cos_w0 + two_sqrt_a_alpha),
                    -2.0 * a * ((a - 1.0) + (a + 1.0) * cos_w0),
                    a * ((a + 1.0) + (a - 1.0) * cos_w0 - two_sqrt_a_alpha),
                ];
                let a_c = [
                    (a + 1.0) - (a - 1.0) * cos_w0 + two_sqrt_a_alpha,
                    2.0 * ((a - 1.0) - (a + 1.0) * cos_w0),
                    (a + 1.0) - (a - 1.0) * cos_w0 - two_sqrt_a_alpha,
                ];
                (b, a_c)
            } else {
                // Peak / Bell
                let b = [1.0 + alpha * a, -2.0 * cos_w0, 1.0 - alpha * a];
                let a_c = [1.0 + alpha / a, -2.0 * cos_w0, 1.0 - alpha / a];
                (b, a_c)
            };

            // Magnitude of H(e^jω): |B(z)| / |A(z)| at z = e^jω
            let omega = 2.0 * PI * f / FS;
            let z1_re = omega.cos();
            let z1_im = -omega.sin();
            let z2_re = (2.0 * omega).cos();
            let z2_im = -(2.0 * omega).sin();

            let num_re = b[0] + b[1] * z1_re + b[2] * z2_re;
            let num_im = b[1] * z1_im + b[2] * z2_im;
            let den_re = a_coeffs[0] + a_coeffs[1] * z1_re + a_coeffs[2] * z2_re;
            let den_im = a_coeffs[1] * z1_im + a_coeffs[2] * z2_im;

            let num_mag_sq = num_re * num_re + num_im * num_im;
            let den_mag_sq = den_re * den_re + den_im * den_im;
            if den_mag_sq < 1e-30 { return 0.0; }
            20.0 * (num_mag_sq / den_mag_sq).sqrt().log10()
        }).collect();

        band_gains.push(band_db);
    }

    // Convert dB gains to SVG paths
    let db_to_y = |db: f64| -> f64 {
        let clamped = db.clamp(GAIN_MIN_DB, GAIN_MAX_DB);
        CANVAS_H * (1.0 - (clamped - GAIN_MIN_DB) / (GAIN_MAX_DB - GAIN_MIN_DB))
    };

    let freq_to_x = |f: f64| -> f64 {
        CANVAS_W * (f / FREQ_MIN).log10() / (FREQ_MAX / FREQ_MIN).log10()
    };

    let make_path = |gains: &[f64]| -> String {
        let mut path = String::new();
        for (i, (&f, &db)) in freqs.iter().zip(gains.iter()).enumerate() {
            let x = freq_to_x(f);
            let y = db_to_y(db);
            if i == 0 {
                path.push_str(&format!("M {:.1},{:.1}", x, y));
            } else {
                path.push_str(&format!(" L {:.1},{:.1}", x, y));
            }
        }
        path
    };

    // Per-band curves
    let band_paths: Vec<String> = band_gains.iter().map(|g| make_path(g)).collect();

    // Total curve: sum dB gains (product of linear gains)
    let total_gains: Vec<f64> = (0..N_POINTS)
        .map(|i| band_gains.iter().map(|bg| bg[i]).sum())
        .collect();
    let total_path = make_path(&total_gains);

    (total_path, band_paths)
}
```

- [ ] **Step 2: Add Slint model for eq-band-curves**

In `lib.rs`, add a `VecModel<SharedString>` for `eq_band_curves`:

```rust
let eq_band_curves = Rc::new(VecModel::from(Vec::<SharedString>::new()));
window.set_eq_band_curves(ModelRc::from(eq_band_curves.clone()));
block_editor_window.set_eq_band_curves(ModelRc::from(eq_band_curves.clone()));
```

(Same pattern as `multi_slider_points` / `curve_editor_points` initialization — add alongside those lines.)

- [ ] **Step 3: Add property declarations to AppWindow and BlockEditorWindow in Slint**

In `app-window.slint`, for `AppWindow` and `BlockEditorWindow`:
```slint
in property <string> eq-total-curve;
in property <[string]> eq-band-curves;
```

(This was already partially done in Task 5 — verify these are present.)

- [ ] **Step 4: Compute and push curves when block editor opens**

Find the section in `lib.rs` where `build_curve_editor_points` is called (triggered when block is selected). Immediately after, add:

```rust
let pts = build_curve_editor_points(&editor_data.effect_type, &editor_data.model_id, &editor_data.params);
let (total, bands) = compute_eq_curves(&pts);
window.set_eq_total_curve(total.into());
eq_band_curves.set_vec(bands.into_iter().map(SharedString::from).collect());
// (Also set for block_editor_window)
block_editor_window.set_eq_total_curve(total_again.into()); // recompute or clone before move
```

Note: compute curves before moving `pts` into the model, or clone the data. Adjust as needed.

- [ ] **Step 5: Recompute curves when parameter changes**

Find `update-block-parameter-number` handler in `lib.rs`. After updating the parameter (existing code), add:

```rust
// If this is a CurveEditor block, recompute and push curves
if let Some(data) = &*editor_data_ref.borrow() {
    let pts = build_curve_editor_points(&data.effect_type, &data.model_id, &data.params);
    if !pts.is_empty() {
        let (total, bands) = compute_eq_curves(&pts);
        window.set_eq_total_curve(total.into());
        eq_band_curves.set_vec(bands.into_iter().map(SharedString::from).collect());
        block_editor_window.set_eq_total_curve(total_again.into());
    }
}
```

The exact location and variable names depend on the existing handler structure — grep for `update-block-parameter-number` in lib.rs to find the right closure.

- [ ] **Step 6: Build and verify zero warnings**

```bash
cargo build -p adapter-gui 2>&1 | grep "^error\|^warning" | grep -v "unused_imports\|dead_code" | head -20
```

Fix any errors. The goal: `Finished` with zero warnings.

- [ ] **Step 7: Commit**

```bash
git add crates/adapter-gui/src/lib.rs crates/adapter-gui/ui/app-window.slint crates/adapter-gui/ui/touch_main.slint
git commit -m "feat: compute and push EQ frequency response curves to Slint"
```

---

## Task 7: Final build check and PR

- [ ] **Step 1: Full build with zero warnings**

```bash
cargo build 2>&1 | grep "^error\|^warning" | head -20
```

Fix any warnings. The project rule is zero warnings.

- [ ] **Step 2: Commit anything remaining**

```bash
git status
# If anything unstaged:
git add -p
git commit -m "fix: address remaining warnings and polish"
```

- [ ] **Step 3: Create PR**

```bash
git push origin feature/issue-156
gh pr create --base develop --title "feat: add parametric and graphic EQ widgets (#156)" --body "$(cat <<'EOF'
## Summary

- Interactive `CurveEditorControl` Slint widget with draggable points and Q wing handles for parametric EQ blocks
- `MultiSliderControl` Slint widget for 31-band graphic EQ (ZamGEQ31)
- Frequency response curves computed in Rust (biquad magnitude response) and displayed as SVG paths
- Three Band EQ redesigned with proper shelf/peak biquad parameters (replaces percentage-based model)
- ZamEQ2, TAP Equalizer, TAP Equalizer/BW use `CurveEditorControl`
- ZamGEQ31 uses `MultiSliderControl`

## Test plan

- [ ] Open a filter block (Three Band EQ) → EQ widget appears instead of knobs
- [ ] Drag a control point vertically → gain parameter updates
- [ ] Drag a control point horizontally → frequency parameter updates
- [ ] Drag Q wing handle → Q/bandwidth parameter updates
- [ ] Frequency response curve updates in real-time while dragging
- [ ] Open ZamGEQ31 → MultiSlider widget with 31 sliders appears
- [ ] Drag a slider → parameter updates
- [ ] Build passes with zero warnings

Closes #156
EOF
)"
```

- [ ] **Step 4: Share checkout command with user**

At the end, include: `git checkout feature/issue-156 && git pull`

---

## Self-Review Notes

**Spec coverage check:**
- ✅ `ParametricEqControl` widget with draggable points — Task 2
- ✅ Wing handles for Q — Task 2  
- ✅ Colored bands, semi-transparent individual curves, white total curve — Task 2 + Task 6
- ✅ `GraphicEqControl` with vertical sliders — Task 3
- ✅ Three Band EQ redesigned — already done
- ✅ All 5 EQ blocks using appropriate widget — already done
- ✅ Real-time visual feedback — Task 6 (recompute on parameter change)

**Known simplifications (acceptable for MVP):**
- The `is_low_shelf`/`is_high_shelf` detection in `compute_eq_curves` uses group name heuristic ("Low Shelf", "High Shelf"). This works for the current models but is fragile. A follow-up could add a `band_kind: string` field to `CurveEditorPoint`.
- The Q handle visual offset formula (`0.08 / width_value`) is a visual approximation, not the true -3dB bandwidth. Good enough for interaction.
- `parent.parent.width` depth in Slint — verify the nesting level is correct for each component context.
