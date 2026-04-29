//! Image asset loaders for thumbnails and screenshots — lifted out of
//! project_view.rs so the parent module stays under the size cap.

pub(crate) fn load_thumbnail_image(effect_type: &str, model_id: &str) -> (slint::Image, bool, f32, f32) {
    use std::cell::RefCell;
    use std::collections::HashMap;

    thread_local! {
        static CACHE: RefCell<HashMap<(String, String), (slint::Image, f32, f32)>> = RefCell::new(HashMap::new());
    }

    let key = (effect_type.to_string(), model_id.to_string());

    let cached = CACHE.with(|c| c.borrow().get(&key).cloned());
    if let Some((img, w, h)) = cached {
        return (img, true, w, h);
    }

    match crate::thumbnails::thumbnail_png(effect_type, model_id) {
        Some(png_bytes) => {
            match image::load_from_memory_with_format(&png_bytes, image::ImageFormat::Png) {
                Ok(img) => {
                    let rgba = img.to_rgba8();
                    let w = rgba.width() as f32;
                    let h = rgba.height() as f32;
                    let buffer = slint::SharedPixelBuffer::<slint::Rgba8Pixel>::clone_from_slice(
                        rgba.as_raw(),
                        rgba.width(),
                        rgba.height(),
                    );
                    let slint_img = slint::Image::from_rgba8(buffer);
                    CACHE.with(|c| c.borrow_mut().insert(key, (slint_img.clone(), w, h)));
                    (slint_img, true, w, h)
                }
                Err(e) => {
                    log::warn!("Failed to decode thumbnail for {}/{}: {}", effect_type, model_id, e);
                    (slint::Image::default(), false, 0.0, 0.0)
                }
            }
        }
        None => (slint::Image::default(), false, 0.0, 0.0)
    }
}

pub(crate) fn load_screenshot_image(effect_type: &str, model_id: &str) -> (slint::Image, bool) {
    match crate::plugin_info::screenshot_png(effect_type, model_id) {
        Some(png_bytes) => {
            match image::load_from_memory_with_format(&png_bytes, image::ImageFormat::Png) {
                Ok(img) => {
                    let rgba = img.to_rgba8();
                    let buffer = slint::SharedPixelBuffer::<slint::Rgba8Pixel>::clone_from_slice(
                        rgba.as_raw(),
                        rgba.width(),
                        rgba.height(),
                    );
                    (slint::Image::from_rgba8(buffer), true)
                }
                Err(e) => {
                    log::warn!(
                        "Failed to decode screenshot for {}/{}: {}",
                        effect_type,
                        model_id,
                        e
                    );
                    (slint::Image::default(), false)
                }
            }
        }
        None => (slint::Image::default(), false),
    }
}
