//! #398 — overlays that hang off a bar must paint over the content declared
//! after it, and a brand without a logo must draw nothing.
//!
//! Slint paints siblings in declaration order unless `z` says otherwise; a
//! header declared before the page body has its hover labels covered by that
//! body (the LOOPER label cut in half by the meter band).

/// The body of the element that opens on `marker`, up to its closing brace.
fn element_body<'a>(source: &'a str, marker: &str) -> &'a str {
    let start = source
        .find(marker)
        .unwrap_or_else(|| panic!("`{marker}` not found"));
    let open = start + marker.len();
    let mut depth = 1;
    for (offset, ch) in source[open..].char_indices() {
        match ch {
            '{' => depth += 1,
            '}' => {
                depth -= 1;
                if depth == 0 {
                    return &source[open..open + offset];
                }
            }
            _ => {}
        }
    }
    panic!("`{marker}` never closes");
}

/// The `z` the element declares at its own level (not a child's).
fn own_z(body: &str) -> Option<f32> {
    let mut depth = 0;
    for line in body.lines() {
        let trimmed = line.trim();
        if depth == 0 {
            if let Some(value) = trimmed.strip_prefix("z:") {
                return value.trim().trim_end_matches(';').parse().ok();
            }
        }
        depth += line.matches('{').count() as i32;
        depth -= line.matches('}').count() as i32;
    }
    None
}

#[test]
fn every_bar_with_hover_labels_paints_over_the_page_below_it() {
    let bars = [
        (
            "chain_row.slint",
            include_str!("../ui/pages/chain_row.slint"),
            "ChainRowHeader {",
        ),
        (
            "project_chains.slint",
            include_str!("../ui/pages/project_chains.slint"),
            "top-bar := Rectangle {",
        ),
        (
            "compact_chain_view.slint",
            include_str!("../ui/pages/compact_chain_view.slint"),
            "CompactChainViewHeader {",
        ),
        (
            "block_panel_editor.slint",
            include_str!("../ui/pages/block_panel_editor.slint"),
            "BlockPanelHeader {",
        ),
    ];
    for (file, source, marker) in bars {
        let z = own_z(element_body(source, marker));
        assert!(
            z.is_some_and(|z| z >= 10.0),
            "{file}: `{marker}` must declare z >= 10 so its hover labels paint over the page, got {z:?}"
        );
    }
}

#[test]
fn a_brand_without_a_logo_draws_nothing() {
    let source = include_str!("../ui/components/brand_logo.slint");
    let body = element_body(source, "export component BrandLogo inherits Image {");
    assert!(
        body.lines().any(|l| l.trim() == "visible: root.has-logo;"),
        "BrandLogo must hide itself when the brand has no logo"
    );
    assert!(
        body.contains(": @image-url(\"\");"),
        "the last fallback must be an empty image, never another brand's logo"
    );
}

#[test]
fn the_compact_row_falls_back_to_the_type_icon_when_the_brand_has_no_logo() {
    let source = include_str!("../ui/pages/compact_block_row.slint");
    assert!(
        source.contains("if !brand-logo.has-logo : EffectTypeIcon {"),
        "a brand with no logo (e.g. klon) must show the type icon, not an empty slot"
    );
}
