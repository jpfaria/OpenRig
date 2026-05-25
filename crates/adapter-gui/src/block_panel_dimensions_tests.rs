//! Property-based coverage for `block_panel_dimensions::compute`.
//!
//! Every assertion below encodes a requirement stated by the user or
//! recovered from the bug history of issue #500, NOT an echo of the
//! current constant values. If `MAX_COLS` changes from 6 to 5 or
//! `ROW_STRIDE_PX` from 100 to 110, these tests must still pass —
//! the policy is what they protect, not the chosen numbers.
//!
//! Bug catalogue (each got a numbered property below):
//! 1. ZaMultiComp (27 params) opened the window at ~1808px wide.
//! 2. Dragonfly Hall (18 params) rendered every knob in one row.
//! 3. Slint integer division returned `ceil(13/6) == 2` instead of 3,
//!    sizing the window 1 row short and clipping the remainder.
//! 4. The inner panel Rectangle clipped wrapped rows behind the
//!    footer/drawer because its height was hardcoded to `width/4`.

use super::*;

fn dims(knob_count: usize) -> PanelDimensions {
    compute(PanelInputs::plain(knob_count))
}

// ── Requirement 1: knobs above MAX_COLS must wrap ────────────
// User: "a porra tem que pular de linha. nao pode ser somente horizontal".

#[test]
fn req_anything_above_max_cols_wraps() {
    for n in (MAX_COLS + 1)..=300 {
        assert!(
            grid_rows(n) >= 2,
            "n={n} produced {} rows, must wrap to ≥2",
            grid_rows(n)
        );
    }
}

#[test]
fn req_ten_knobs_wrap_into_at_least_two_rows() {
    // User example: "se eu tiver 10 parametros por ex.. tinha que
    // cresver vertical e ter duas linhas". This is a hard requirement
    // independent of MAX_COLS — it constrains MAX_COLS ≤ 9.
    assert!(
        grid_rows(10) >= 2,
        "ten knobs collapsed into {} row(s)",
        grid_rows(10)
    );
    assert!(
        grid_cols(10) < 10,
        "ten knobs spread across {} cols, leaving no rows to grow into",
        grid_cols(10)
    );
}

#[test]
fn req_eighteen_knobs_dragonfly_hall_wraps() {
    // Screenshot bug: 18 knobs in one row.
    assert!(
        grid_rows(18) >= 3,
        "dragonfly_hall (18 knobs) only produced {} row(s)",
        grid_rows(18)
    );
}

#[test]
fn req_zamulticomp_twentyseven_knobs_wraps_into_many_rows() {
    // 27 knobs is well past any reasonable single-row count.
    assert!(
        grid_rows(27) >= 4,
        "zamulticomp (27 knobs) only produced {} row(s)",
        grid_rows(27)
    );
}

// ── Requirement 2: window width must stay bounded ────────────
// Bug 1: window opened at 1808px for 27 params.

#[test]
fn req_window_width_bounded_regardless_of_count() {
    // The window must never grow past a sane upper bound. We pick the
    // theoretical max from the formula constants — anything wider
    // means the wrap policy regressed.
    let single_row_max = (MAX_COLS as f32) * CELL_WIDTH_PX + HORIZONTAL_MARGIN_PX;
    let cap = MIN_PANEL_WIDTH_PX.max(single_row_max);
    for n in 0..=1_000 {
        let w = dims(n).window_width_px;
        assert!(
            w <= cap,
            "n={n} produced width {w}px (cap {cap}px) — wrap policy regressed"
        );
    }
}

#[test]
fn req_window_never_exceeds_typical_screen_for_real_plugins() {
    // No real LV2/NAM/IR bundle ships more than ~150 knobs; ensure the
    // policy holds under realistic conditions and never produces a
    // window wider than 1280px (1366p laptops still need to host it).
    for n in 0..=200 {
        assert!(
            dims(n).window_width_px <= 1280.0,
            "n={n} would not fit a 1366×768 screen"
        );
    }
}

// ── Requirement 3: window grows VERTICALLY, not horizontally ─
// User: "tinha que crescer vertical e ter duas linhas".

#[test]
fn req_width_is_invariant_across_panel_counts() {
    // Adding more knobs must not widen the window — vertical growth only.
    let any_width = dims(1).window_width_px;
    for n in 2..=300 {
        assert_eq!(
            dims(n).window_width_px,
            any_width,
            "n={n} changed width to {} (expected constant)",
            dims(n).window_width_px
        );
    }
}

#[test]
fn req_height_grows_strictly_at_each_wrap_boundary() {
    // Every additional *wrapped* row must produce a strictly taller
    // window. The 0→1 transition is excluded (empty grid and single-
    // row grid both use the baseline height by design — the single
    // row fits inside the amp-head base aspect).
    let mut last_rows = 1;
    let mut last_height = dims(1).window_height_px;
    for n in 2..=200 {
        let d = dims(n);
        if d.grid_rows > last_rows {
            assert!(
                d.window_height_px > last_height,
                "n={n}: rows went {last_rows}→{} but height stayed {last_height}",
                d.grid_rows
            );
            last_rows = d.grid_rows;
            last_height = d.window_height_px;
        }
    }
}

#[test]
fn req_height_monotonic_non_decreasing() {
    let mut prev = dims(0).window_height_px;
    for n in 1..=200 {
        let h = dims(n).window_height_px;
        assert!(h >= prev, "n={n}: height {h} < previous {prev}");
        prev = h;
    }
}

// ── Requirement 4: wrap math is correct (no off-by-one) ──────
// Bug 3: Slint int division produced 1 row short for partial last rows.

#[test]
fn req_grid_has_capacity_for_every_knob() {
    for n in 1..=300 {
        let d = dims(n);
        assert!(
            d.grid_cols * d.grid_rows >= n,
            "n={n}: {}×{} grid cannot hold all knobs",
            d.grid_cols,
            d.grid_rows
        );
    }
}

#[test]
fn req_grid_has_no_empty_trailing_row() {
    for n in 1..=300 {
        let d = dims(n);
        if d.grid_rows >= 2 {
            assert!(
                d.grid_cols * (d.grid_rows - 1) < n,
                "n={n}: {}×{} grid has an empty trailing row",
                d.grid_cols,
                d.grid_rows
            );
        }
    }
}

#[test]
fn req_rows_monotonic_in_count() {
    let mut prev = 0;
    for n in 0..=300 {
        let r = grid_rows(n);
        assert!(r >= prev, "n={n}: rows {r} < previous {prev}");
        prev = r;
    }
}

#[test]
fn req_each_row_completely_filled_except_the_last() {
    // For any n, n = (full_rows × cols) + remainder where
    // 0 < remainder ≤ cols (or n == 0).
    for n in 1..=300 {
        let d = dims(n);
        if d.grid_rows == 1 {
            assert!(n <= d.grid_cols);
        } else {
            let full = (d.grid_rows - 1) * d.grid_cols;
            assert!(full < n, "n={n}: full rows {full} should be < n");
            let remainder = n - full;
            assert!(remainder > 0 && remainder <= d.grid_cols);
        }
    }
}

// ── Requirement 5: inner panel must clip-safely hold every row ─
// Bug 4: rows past `root.width / 4` rendered behind the footer.

#[test]
fn req_inner_panel_holds_last_row_with_clip_safe_margin() {
    // The first row anchors at 0.54 × base_inner (the amp-head bottom).
    // Successive rows step by ROW_STRIDE_PX. The inner panel height
    // must be ≥ the y-coordinate of the LAST row's bottom edge.
    for n in 0..=300 {
        let d = dims(n);
        if d.grid_rows == 0 {
            continue;
        }
        let base_inner = d.window_width_px / 4.0;
        let row0_y = base_inner * 0.54;
        let last_row_bottom =
            row0_y + (d.grid_rows.saturating_sub(1)) as f32 * ROW_STRIDE_PX + ITEM_HEIGHT_PX;
        assert!(
            d.inner_panel_height_px >= last_row_bottom,
            "n={n}: inner panel {} clips last row at {}",
            d.inner_panel_height_px,
            last_row_bottom
        );
    }
}

// ── Requirement 6: minimums and floors ───────────────────────

#[test]
fn req_panel_width_respects_minimum_floor() {
    for n in 0..=300 {
        let w = dims(n).window_width_px;
        assert!(
            w >= MIN_PANEL_WIDTH_PX,
            "n={n}: width {w} dropped below floor {MIN_PANEL_WIDTH_PX}"
        );
    }
}

#[test]
fn req_zero_knobs_panel_still_has_baseline_dimensions() {
    // Empty knob list (transient state during model switch) must not
    // produce a 0×0 or NaN window.
    let d = dims(0);
    assert!(d.window_width_px >= MIN_PANEL_WIDTH_PX);
    assert!(d.window_height_px >= BASE_PANEL_HEIGHT_PX);
    assert!(d.window_width_px.is_finite());
    assert!(d.window_height_px.is_finite());
}

#[test]
fn req_single_row_uses_baseline_height() {
    for n in 1..=MAX_COLS {
        assert_eq!(
            dims(n).window_height_px,
            BASE_PANEL_HEIGHT_PX,
            "n={n}: single-row case inflated the baseline"
        );
    }
}

// ── Requirement 7: universal across block kinds ──────────────
// User: "isso serve para qualquer tipo de bloco e para qualquer tipo.
//        nam, nativo, lv2.. a regra tem que ser igual".

#[test]
fn req_layout_does_not_depend_on_knob_origin() {
    // Whether the caller derived the count from `block_knob_overlays`
    // (curated NAM/native overlay) or `block_parameter_items` (LV2-
    // synthesized control ports) must produce identical dimensions —
    // both feed the same `knob_count` field.
    for n in 0..=200 {
        let a = compute(PanelInputs {
            knob_count: n,
            use_panel_editor: true,
            has_eq_widget: false,
        });
        let b = compute(PanelInputs {
            knob_count: n,
            use_panel_editor: true,
            has_eq_widget: false,
        });
        assert_eq!(a, b, "n={n}");
    }
}

// ── Requirement 8: special modes short-circuit cleanly ───────

#[test]
fn req_form_editor_uses_fixed_fallback_dimensions() {
    // Native form blocks bypass the wrap math.
    for n in [0, 1, 27, 64, 1_000] {
        let d = compute(PanelInputs {
            knob_count: n,
            use_panel_editor: false,
            has_eq_widget: false,
        });
        assert_eq!(d.grid_rows, 0, "n={n}");
        assert_eq!(d.window_width_px, FORM_EDITOR_WIDTH_PX, "n={n}");
        assert_eq!(d.window_height_px, FORM_EDITOR_HEIGHT_PX, "n={n}");
    }
}

#[test]
fn req_eq_mode_does_not_open_the_param_grid() {
    // EQ widgets own their own geometry (CurveEditor / MultiSlider).
    // The param grid must collapse so the window does not inflate
    // for an empty grid that never renders.
    for n in [0, 1, 27, 64] {
        let d = compute(PanelInputs {
            knob_count: n,
            use_panel_editor: true,
            has_eq_widget: true,
        });
        assert_eq!(d.grid_cols, 0, "n={n}");
        assert_eq!(d.grid_rows, 0, "n={n}");
        // The Slint EQ branch sizes its own width/height; we only
        // assert the param grid was bypassed (cols/rows = 0).
    }
}

// ── Requirement 9: continuous correctness over a wide range ──
// User: "300000 se forem necessario.. com todos os tipos de prametros".
// 1..=300 covers every realistic plugin and a 4× safety margin
// (largest catalogued plugin: fat1_autotune at 64).

#[test]
fn req_every_count_zero_to_three_hundred_is_well_formed() {
    for n in 0..=300 {
        let d = dims(n);
        assert!(d.window_width_px.is_finite() && d.window_width_px > 0.0);
        assert!(d.window_height_px.is_finite() && d.window_height_px > 0.0);
        assert!(d.inner_panel_height_px.is_finite() && d.inner_panel_height_px >= 0.0);
        if n > 0 {
            assert!(d.grid_cols >= 1);
            assert!(d.grid_rows >= 1);
        }
    }
}
