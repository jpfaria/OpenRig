use std::path::Path;
fn main() {
    plugin_loader::registry::init(Path::new(
        "/Users/joao.faria/Projetos/github.com/jpfaria/OpenRig-plugins/plugins/source",
    ));
    if let Ok(entries) = project::catalog::supported_block_models("pitch") {
        let visible: Vec<_> = entries
            .iter()
            .filter(|e| e.supported_instruments.iter().any(|i| i == "voice"))
            .collect();
        println!("pitch voice-visible: {}", visible.len());
        for e in &visible {
            println!("  {} brand={}", e.model_id, e.brand);
        }
    }
    if let Ok(entries) = project::catalog::supported_block_models("modulation") {
        let voice: Vec<_> = entries
            .iter()
            .filter(|e| {
                e.supported_instruments.iter().any(|i| i == "voice")
                    && (e.model_id.contains("larynx") || e.model_id.contains("harmless"))
            })
            .collect();
        println!("modulation voice-related: {}", voice.len());
        for e in &voice {
            println!("  {} brand={}", e.model_id, e.brand);
        }
    }
}
