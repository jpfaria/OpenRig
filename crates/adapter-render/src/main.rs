//! `openrig-render` binary entry — wires argv → `render()` → stdout.

use std::path::PathBuf;
use std::process::ExitCode;

use adapter_render::{cli::parse_render_args, render};

fn main() -> ExitCode {
    let argv: Vec<String> = std::env::args().collect();

    let args = match parse_render_args(&argv) {
        Ok(a) => a,
        Err(e) => {
            eprintln!("openrig-render: {e}");
            return ExitCode::from(2);
        }
    };

    // Populate the model registries before loading the chain. Mirrors the
    // GUI bootstrap in adapter-gui::desktop_app — without this, disk-package
    // models (NAM captures, IR cabs, LV2 plugins) aren't visible to the
    // schema lookup, and the preset loader silently drops every block that
    // references one. Issue #552.
    engine::native_registry::register_all_natives();
    // Same `config.yaml` lookup the GUI uses (CWD-first so dev runs see the
    // checked-in `plugins_root`; falls back to the bundled data root, which
    // is where the .app/.deb/.msi installers drop their config next to the
    // binary). The `OPENRIG_PLUGINS_ROOT` env var still overrides everything
    // — see `plugin_loader::plugins_root_from_config`.
    let bundled_root = infra_filesystem::detect_data_root().join("plugins");
    let config_path = {
        let cwd_config = PathBuf::from("config.yaml");
        if cwd_config.exists() {
            cwd_config
        } else {
            infra_filesystem::detect_data_root().join("config.yaml")
        }
    };
    let user_root = plugin_loader::plugins_root_from_config(&config_path);
    plugin_loader::registry::init_many(&[bundled_root, user_root]);

    match render(&args) {
        Ok(summary) => {
            println!(
                "openrig-render: wrote {} frames @ {} Hz to {}",
                summary.frames_written,
                summary.sample_rate_hz,
                summary.output.display(),
            );
            ExitCode::SUCCESS
        }
        Err(e) => {
            eprintln!("openrig-render: {e}");
            ExitCode::FAILURE
        }
    }
}
