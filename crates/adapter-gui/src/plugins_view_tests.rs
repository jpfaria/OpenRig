use application::plugin_library::entries::PluginLibraryEntry;
use application::plugin_library::PluginOrigin;
use plugin_loader::manifest::{BlockType, NamArchitecture};

use super::*;

fn entry(id: &str, name: &str, block_type: BlockType, origin: PluginOrigin) -> PluginLibraryEntry {
    PluginLibraryEntry {
        plugin_id: id.into(),
        display_name: name.into(),
        brand: Some("Marshall".into()),
        block_type,
        backend: "nam".into(),
        architecture: Some(NamArchitecture::A2),
        origin,
        captures: 4,
        editable: true,
        tone_ids: vec![],
        updated_at: None,
        versions: vec![],
    }
}

fn library() -> Vec<PluginLibraryEntry> {
    vec![
        entry("plexi", "Plexi", BlockType::Amp, PluginOrigin::Tone3000),
        entry(
            "v30",
            "V30 4x12",
            BlockType::Cab,
            PluginOrigin::PluginsFolder,
        ),
        entry(
            "ts",
            "Screamer",
            BlockType::GainPedal,
            PluginOrigin::Tone3000,
        ),
    ]
}

fn names(view: &PluginsView) -> Vec<&str> {
    view.rows.iter().map(|r| r.name.as_str()).collect()
}

#[test]
fn with_no_filter_every_plugin_is_listed() {
    let view = plugins_view(&library(), &PluginsFilter::default());
    assert_eq!(names(&view), vec!["Plexi", "V30 4x12", "Screamer"]);
    assert_eq!(view.total, 3);
}

#[test]
fn the_origin_filter_keeps_only_that_origin() {
    let filter = PluginsFilter {
        origin: OriginFilter::Tone3000,
        ..Default::default()
    };
    assert_eq!(
        names(&plugins_view(&library(), &filter)),
        vec!["Plexi", "Screamer"]
    );
    let filter = PluginsFilter {
        origin: OriginFilter::PluginsFolder,
        ..Default::default()
    };
    assert_eq!(names(&plugins_view(&library(), &filter)), vec!["V30 4x12"]);
}

#[test]
fn the_type_filter_keeps_only_that_type() {
    let filter = PluginsFilter {
        effect_type: block_type_to_effect_type(BlockType::Cab).into(),
        ..Default::default()
    };
    assert_eq!(names(&plugins_view(&library(), &filter)), vec!["V30 4x12"]);
}

#[test]
fn the_search_matches_name_or_brand_ignoring_case() {
    let filter = PluginsFilter {
        query: "  SCREAM ".into(),
        ..Default::default()
    };
    assert_eq!(names(&plugins_view(&library(), &filter)), vec!["Screamer"]);
    let filter = PluginsFilter {
        query: "marshall".into(),
        ..Default::default()
    };
    assert_eq!(plugins_view(&library(), &filter).rows.len(), 3);
}

#[test]
fn the_type_choices_are_only_the_types_the_user_has() {
    let view = plugins_view(&library(), &PluginsFilter::default());
    let keys: Vec<&str> = view.types.iter().map(|t| t.key.as_str()).collect();
    assert_eq!(keys.len(), 3);
    for block_type in [BlockType::Amp, BlockType::Cab, BlockType::GainPedal] {
        assert!(keys.contains(&block_type_to_effect_type(block_type)));
    }
    assert!(view.types.iter().all(|t| !t.label.is_empty()));
}

#[test]
fn a_row_marks_tone3000_and_reads_backend_and_architecture() {
    let view = plugins_view(&library(), &PluginsFilter::default());
    let plexi = &view.rows[0];
    assert!(plexi.tone3000);
    assert!(!view.rows[1].tone3000);
    assert_eq!((plexi.backend.as_str(), plexi.arch.as_str()), ("NAM", "A2"));
    assert_eq!(plexi.captures, 4);
    assert_eq!(plexi.brand, "Marshall");
    assert!(!plexi.type_label.is_empty());
}

#[test]
fn origin_segments_map_in_order() {
    assert_eq!(OriginFilter::from_index(0), OriginFilter::All);
    assert_eq!(OriginFilter::from_index(1), OriginFilter::PluginsFolder);
    assert_eq!(OriginFilter::from_index(2), OriginFilter::Tone3000);
    assert_eq!(OriginFilter::from_index(9), OriginFilter::All);
}
