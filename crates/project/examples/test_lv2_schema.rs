use std::path::Path;
fn main() {
    plugin_loader::registry::init(Path::new(
        "/Users/joao.faria/Projetos/github.com/jpfaria/OpenRig-plugins/plugins/source",
    ));
    for id in &[
        "lv2_dragonfly_room",
        "lv2_tap_chorus_flanger",
        "lv2_zamcomp",
        "ir_gibson_j45",
    ] {
        let effect = if id.starts_with("ir_gibson") {
            "body"
        } else if id.contains("dragonfly") {
            "reverb"
        } else if id.contains("chorus") || id.contains("flanger") {
            "modulation"
        } else {
            "dynamics"
        };
        match project::block::schema_for_block_model(effect, id) {
            Ok(s) => {
                println!(
                    "{} -> {} params: {:?}",
                    id,
                    s.parameters.len(),
                    s.parameters
                        .iter()
                        .map(|p| p.path.as_str())
                        .collect::<Vec<_>>()
                );
            }
            Err(e) => println!("{} ERROR: {}", id, e),
        }
    }
}
