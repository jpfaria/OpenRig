use std::path::Path;
fn main() {
    plugin_loader::registry::init(Path::new(
        "/Users/joao.faria/Projetos/github.com/jpfaria/OpenRig-plugins/plugins/source",
    ));
    if let Ok(s) = project::block::schema_for_block_model("preamp", "nam_diezel_vh4") {
        for p in &s.parameters {
            use block_core::param::ParameterDomain;
            let kind = match &p.domain {
                ParameterDomain::Bool => "BOOL".to_string(),
                ParameterDomain::Enum { options } =>
                    format!("ENUM {:?}", options.iter().map(|o| o.label.as_str()).collect::<Vec<_>>()),
                ParameterDomain::FloatRange { min, max, step } =>
                    format!("FLOAT [{min}..{max}] step={step}"),
                _ => format!("{:?}", p.domain),
            };
            println!("  {:<25} {} ({})", p.path, kind, p.label);
        }
    }
}
