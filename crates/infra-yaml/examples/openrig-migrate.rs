//! Manual backward-compat check (#450). Feed it a legacy/new project file
//! or a standalone legacy preset file and see what the transparent loader
//! does — no UI needed.
//!
//!   cargo run -p infra-yaml --example openrig-migrate -- /path/project.yaml
//!   cargo run -p infra-yaml --example openrig-migrate -- /path/my-preset.yaml
//!
//! A legacy project is auto-migrated to a sibling `project.openrig`
//! (+ one-time `<file>.bak`); a new `project.openrig` is loaded as-is;
//! anything else is tried as a legacy preset and converted to a RigPreset.
//! The file is only ever read (legacy backup aside) — nothing destructive.

use std::path::PathBuf;

fn main() -> anyhow::Result<()> {
    let path: PathBuf = match std::env::args().nth(1) {
        Some(p) => PathBuf::from(p),
        None => {
            eprintln!(
                "usage: cargo run -p infra-yaml --example openrig-migrate -- <file>\n\
                 <file> = a legacy/new project file, or a legacy preset file"
            );
            std::process::exit(2);
        }
    };

    match infra_yaml::load_project_any(&path) {
        Ok(rig) => {
            let sibling = path.with_extension("openrig");
            println!("✓ loaded as PROJECT");
            println!("  name:    {:?}", rig.name);
            println!("  inputs:  {}", rig.inputs.len());
            println!("  outputs: {}", rig.outputs.len());
            println!("  presets: {}", rig.presets.len());
            for (name, p) in &rig.presets {
                println!(
                    "    - {name}: {} block(s), volume {}",
                    p.blocks.len(),
                    p.volume
                );
            }
            if sibling.exists() {
                println!("  → migrated/written: {}", sibling.display());
            }
            let bak = PathBuf::from(format!("{}.bak", path.display()));
            if bak.exists() {
                println!("  → legacy backup:    {}", bak.display());
            }
            Ok(())
        }
        Err(project_err) => match infra_yaml::load_legacy_preset_as_rig(&path) {
            Ok((name, preset)) => {
                println!("✓ loaded as legacy PRESET → RigPreset");
                println!("  name:   {name}");
                println!("  blocks: {}", preset.blocks.len());
                println!("  volume: {}", preset.volume);
                println!("  scenes: {} (none ⇒ default scene)", preset.scenes.len());
                Ok(())
            }
            Err(preset_err) => {
                anyhow::bail!(
                    "not a loadable project or preset.\n  as project: {project_err}\n  \
                     as preset:  {preset_err}"
                )
            }
        },
    }
}
