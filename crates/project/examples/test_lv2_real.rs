use std::path::Path;
fn main() {
    plugin_loader::registry::init(Path::new(
        "/Users/joao.faria/Projetos/github.com/jpfaria/OpenRig-plugins/plugins/source",
    ));
    for id in &[
        "lv2_dragonfly_room",
        "lv2_dragonfly_hall",
        "lv2_tap_chorus_flanger",
        "lv2_zamcomp",
    ] {
        let pkg = match plugin_loader::registry::find(id) {
            Some(p) => p,
            None => {
                println!("{} NOT IN REGISTRY", id);
                continue;
            }
        };
        let data_dir = pkg.root.join("data");
        println!("{}: data_dir exists={}", id, data_dir.is_dir());
        if let plugin_loader::manifest::Backend::Lv2 { plugin_uri, .. } = &pkg.manifest.backend {
            match plugin_loader::dispatch::scan_lv2_ports(&data_dir, plugin_uri) {
                Ok(ports) => println!(
                    "  ports: {} (ControlIn count: {})",
                    ports.len(),
                    ports
                        .iter()
                        .filter(|p| matches!(
                            p.role,
                            plugin_loader::dispatch::Lv2PortRole::ControlIn
                        ))
                        .count()
                ),
                Err(e) => println!("  scan ERROR: {}", e),
            }
        }
    }
}
