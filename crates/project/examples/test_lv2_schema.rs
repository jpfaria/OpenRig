//! Dumps schema synthesis for plugins exercising every LV2 port type
//! routed by `synthesize_lv2_parameters` (issue #401). Used to verify
//! toggle/enum/integer detection at the schema layer without booting
//! the full GUI.
use std::path::Path;

fn main() {
    plugin_loader::registry::init(Path::new(
        "/Users/joao.faria/Projetos/github.com/jpfaria/OpenRig-plugins/plugins/source",
    ));

    let cases = [
        ("pitch", "lv2_fat1_autotune"),
        ("mod", "lv2_larynx"),
        ("mod", "lv2_harmless"),
        ("dynamics", "lv2_zamcomp"),
        ("filter", "lv2_fomp_autowah"),
    ];

    for (effect, id) in &cases {
        match project::block::schema_for_block_model(effect, id) {
            Ok(schema) => {
                println!("\n=== {id} ({effect}, {} params) ===", schema.parameters.len());
                for p in &schema.parameters {
                    use block_core::param::{ParameterDomain, ParameterWidget};
                    let kind = match (&p.widget, &p.domain) {
                        (ParameterWidget::Toggle, _) => "TOGGLE".to_string(),
                        (ParameterWidget::Select, _) => match &p.domain {
                            ParameterDomain::Enum { options } => {
                                format!("SELECT [{}]", options.iter().map(|o| o.label.as_str()).collect::<Vec<_>>().join("/"))
                            }
                            _ => "SELECT".to_string(),
                        },
                        (_, ParameterDomain::FloatRange { min, max, step }) => {
                            if *step > 0.0 {
                                format!("INT step={step}")
                            } else {
                                format!("FLOAT[{min}..{max}]")
                            }
                        }
                        _ => format!("{:?}", p.widget),
                    };
                    println!("  {:>32}  {}  ({})", kind, p.path, p.label);
                }
            }
            Err(e) => println!("\n=== {id} ({effect}) ERROR: {e}"),
        }
    }
}
