use std::path::Path;

fn main() {
    plugin_loader::registry::init(Path::new("/Users/joao.faria/Projetos/github.com/jpfaria/OpenRig-plugins/plugins/source"));
    let total = plugin_loader::registry::len();
    let natives = plugin_loader::registry::native_count();
    println!("registry: {} total, {} native, {} disk", total, natives, total - natives);
    
    let mod_pkgs = plugin_loader::registry::packages_for(plugin_loader::manifest::BlockType::Mod);
    println!("BlockType::Mod packages: {}", mod_pkgs.len());
    for p in mod_pkgs.iter().take(3) {
        println!("  - {} ({})", p.manifest.id, p.manifest.display_name);
    }
    
    let result = project::catalog::supported_block_models("modulation");
    match result {
        Ok(items) => {
            println!("supported_block_models(modulation): {} items", items.len());
            for it in items.iter().take(10) {
                println!("  - {} | brand={} | type={}", it.model_id, it.brand, it.type_label);
            }
        }
        Err(e) => println!("ERROR: {}", e),
    }
}
