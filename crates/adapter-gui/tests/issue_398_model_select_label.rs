//! #398 — a block's model field names the model the way the catalog writes
//! it ("Fender 68 Custom Deluxe"), never with the brand's raw id in front
//! ("fender 68 Custom Deluxe"). The label is composed in Rust; the field only
//! shows it.

use adapter_gui::{BlockModelPickerItem, CompactBlockItem, CompactChainViewWindow};
use i_slint_backend_testing::ElementHandle;
use slint::{ComponentHandle, ModelRc, SharedString, VecModel};
use std::rc::Rc;

fn fender() -> BlockModelPickerItem {
    BlockModelPickerItem {
        effect_type: "amp".into(),
        model_id: "fender_68_custom_deluxe".into(),
        label: "Fender 68 Custom Deluxe".into(),
        display_name: "68 Custom Deluxe".into(),
        subtitle: SharedString::default(),
        icon_kind: "amp".into(),
        brand: "fender".into(),
        type_label: "NAM/A2".into(),
        panel_bg: slint::Color::from_argb_u8(255, 0, 0, 0),
        panel_text: slint::Color::from_argb_u8(255, 255, 255, 255),
        brand_strip_bg: slint::Color::from_argb_u8(255, 0, 0, 0),
        model_font: SharedString::default(),
        available: true,
        thumbnail_path: SharedString::default(),
    }
}

fn amp_block() -> CompactBlockItem {
    let models = vec![fender()];
    CompactBlockItem {
        chain_index: 0,
        block_index: 0,
        display_label: "AMP".into(),
        model_id: "fender_68_custom_deluxe".into(),
        model_selected_index: 0,
        models: ModelRc::from(Rc::new(VecModel::from(models.clone()))),
        filtered_models: ModelRc::from(Rc::new(VecModel::from(models))),
        enabled: true,
        row_height: 140.0,
        row_y: 12.0,
        ..Default::default()
    }
}

#[test]
fn the_model_field_shows_the_catalog_label_not_the_raw_brand_id() {
    i_slint_backend_testing::init_no_event_loop();
    let w = CompactChainViewWindow::new().unwrap();
    w.set_compact_blocks(ModelRc::from(Rc::new(VecModel::from(vec![amp_block()]))));
    w.show().unwrap();
    assert_eq!(
        ElementHandle::find_by_accessible_label(&w, "fender 68 Custom Deluxe").count(),
        0,
        "the brand's raw id is never put in front of the model name"
    );
    assert!(
        ElementHandle::find_by_accessible_label(&w, "Fender 68 Custom Deluxe")
            .next()
            .is_some(),
        "the field shows the label the catalog composed"
    );
}
