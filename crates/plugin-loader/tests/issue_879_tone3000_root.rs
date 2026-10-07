//! #879: plugins installed from TONE3000 live in their own folder next to the
//! system `config.yaml`, apart from the bundled and user plugin trees, so the
//! plugins path setting never moves or hides them.

use std::path::Path;

use plugin_loader::tone3000_root_from_config;

#[test]
fn the_tone3000_root_sits_next_to_the_config() {
    let root = tone3000_root_from_config(Path::new("/data/OpenRig/config.yaml"));
    assert_eq!(root, Path::new("/data/OpenRig/tone3000"));
}

#[test]
fn a_bare_config_name_resolves_to_the_current_folder() {
    assert_eq!(
        tone3000_root_from_config(Path::new("config.yaml")),
        Path::new("tone3000")
    );
}
