use std::path::Path;
fn main() {
    plugin_loader::registry::init(Path::new(
        "/Users/joao.faria/Projetos/github.com/jpfaria/OpenRig-plugins/plugins/source",
    ));
    let effects = ["preamp", "amp", "cab", "body", "ir", "gain", "delay", "reverb",
        "dynamics", "filter", "wah", "pitch", "modulation", "nam"];
    let mut empty = Vec::new();
    let mut total = 0;
    for effect in &effects {
        if let Ok(entries) = project::catalog::supported_block_models(effect) {
            for entry in &entries {
                total += 1;
                let n = project::block::schema_for_block_model(effect, &entry.model_id)
                    .map(|s| s.parameters.len()).unwrap_or(0);
                if n == 0 { empty.push((effect.to_string(), entry.model_id.clone())); }
            }
        }
    }
    println!("Total: {}, with 0 params: {}", total, empty.len());
    for (e, id) in empty.iter().take(30) {
        println!("  [{}] {}", e, id);
    }

    // Sample NAM with grid + spot-check that 8 standard params are merged
    println!("\n=== nam_boss_ds_2 ===");
    if let Ok(s) = project::block::schema_for_block_model("gain_pedal", "nam_boss_ds_2") {
        println!("  {} params: {:?}", s.parameters.len(),
            s.parameters.iter().map(|p| p.path.as_str()).collect::<Vec<_>>());
    }
    println!("\n=== nam_engl_thunder_50 ===");
    if let Ok(s) = project::block::schema_for_block_model("preamp", "nam_engl_thunder_50") {
        println!("  {} params: {:?}", s.parameters.len(),
            s.parameters.iter().map(|p| p.path.as_str()).collect::<Vec<_>>());
    }
}
