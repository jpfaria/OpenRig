use std::path::Path;

fn main() {
    plugin_loader::registry::init(Path::new(
        "/Users/joao.faria/Projetos/github.com/jpfaria/OpenRig-plugins/plugins/source",
    ));

    for effect in &["pitch", "modulation", "filter", "dynamics", "delay", "reverb", "gain"] {
        if let Ok(entries) = project::catalog::supported_block_models(effect) {
            let visible: Vec<_> = entries
                .iter()
                .filter(|item| item.supported_instruments.iter().any(|i| i == "voice"))
                .collect();
            println!("voice-visible in '{}': {} plugins", effect, visible.len());
        }
    }
}
