//! #251: `resolve_uid_for_model` must always resolve to the plugin's **Audio
//! Module Class** (the audio processor / IComponent), and be stable across
//! repeated calls.
//!
//! Regression: ValhallaSupermassive exposes two factory classes with the SAME
//! name ("ValhallaSupermassive") — the Audio Module Class and the Component
//! Controller Class. The uid cache keyed on name let the controller (inserted
//! last) overwrite the processor, so the first resolve returned the processor
//! but every later call returned the controller. Instantiating IComponent on
//! the controller returns kNoInterface (-1) → the block faulted into bypass.
//!
//! Requires ValhallaSupermassive installed. Skips (passes) when absent.

use vst3_host::{find_vst3_plugin, init_vst3_catalog, resolve_uid_for_model, Vst3Plugin};

const MODEL_ID: &str = "vst3:ValhallaSupermassive:ValhallaSupermassive";
const SR: f64 = 48_000.0;

#[test]
fn resolve_uid_is_stable_and_is_the_audio_module_class() {
    init_vst3_catalog(SR, &[]);
    let Some(entry) = find_vst3_plugin(MODEL_ID) else {
        eprintln!("ValhallaSupermassive not installed — skipping");
        return;
    };
    let bundle = entry.info.bundle_path.clone();

    // Resolve several times — the cache must not drift to a different class.
    let uid1 = resolve_uid_for_model(MODEL_ID).expect("resolve #1");
    let uid2 = resolve_uid_for_model(MODEL_ID).expect("resolve #2");
    let uid3 = resolve_uid_for_model(MODEL_ID).expect("resolve #3");
    assert_eq!(uid1, uid2, "resolve_uid_for_model drifted between calls");
    assert_eq!(uid2, uid3, "resolve_uid_for_model drifted between calls");

    // And the resolved uid MUST be the Audio Module Class (the IComponent
    // processor), never the Component Controller Class.
    let (_lib, classes) = Vst3Plugin::enumerate_classes(&bundle).expect("enumerate");
    let audio = classes
        .iter()
        .find(|c| c.category.contains("Audio Module Class"))
        .expect("plugin has an Audio Module Class");
    assert_eq!(
        uid3, audio.uid,
        "resolved uid must be the Audio Module Class ({}), not another class",
        audio.name
    );
}
