use std::path::Path;

fn main() {
    plugin_loader::registry::init(Path::new("/Users/joao.faria/Projetos/github.com/jpfaria/OpenRig-plugins/plugins/source"));
    let yaml = std::fs::read_to_string("/Users/joao.faria/.openrig/project.yaml").expect("read project.yaml");
    let mut current_type = String::new();
    let mut tested = 0;
    let mut failed = Vec::new();
    for line in yaml.lines() {
        let line = line.trim_start();
        if let Some(rest) = line.strip_prefix("type: ") {
            current_type = rest.trim().to_string();
        } else if let Some(rest) = line.strip_prefix("model: ") {
            let model = rest.trim();
            if current_type == "input" || current_type == "output" {
                continue;
            }
            tested += 1;
            let effect_type = match current_type.as_str() {
                "modulation" => "modulation",
                "dynamics" => "dynamics",
                _ => current_type.as_str(),
            };
            match project::block::schema_for_block_model(effect_type, model) {
                Ok(_) => {}
                Err(e) => {
                    failed.push(format!("{}/{}: {}", current_type, model, e));
                }
            }
        }
    }
    println!("tested {} models", tested);
    println!("failed {}:", failed.len());
    for f in &failed {
        println!("  {}", f);
    }
}
