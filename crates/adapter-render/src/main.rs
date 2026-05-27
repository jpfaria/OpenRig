//! `openrig-render` binary entry — wires argv → `render()` → stdout.

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
