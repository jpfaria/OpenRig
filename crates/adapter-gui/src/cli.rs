//! Minimal command-line argument parser for the desktop binary.
//!
//! Three forms are accepted (any order):
//!
//! * positional path → opens that project file directly, skipping the
//!   launcher.
//! * `--auto-save` → enables auto-save on every change (the save button is
//!   hidden in this mode).
//! * `--fullscreen` → forces the inline (no-child-windows) UI; required on
//!   embedded targets where popping up extra OS windows isn't possible.
//!
//! `argv[0]` is skipped (the binary name). Anything else starting with `-`
//! is silently ignored to leave room for future flags without breaking
//! existing callers.

use std::net::SocketAddr;
use std::path::{Path, PathBuf};

/// Parse the opt-in MCP server flag.
///
/// * `--mcp` → default `127.0.0.1:4123`
/// * `--mcp=ADDR` → that socket address (invalid value → `None`, logged)
/// * absent → `None` (server not started; zero overhead)
pub fn parse_mcp_addr(args: &[&str]) -> Option<SocketAddr> {
    for arg in args.iter().skip(1) {
        if *arg == "--mcp" {
            return Some("127.0.0.1:4123".parse().expect("valid default mcp addr"));
        }
        if let Some(rest) = arg.strip_prefix("--mcp=") {
            return match rest.parse() {
                Ok(addr) => Some(addr),
                Err(_) => {
                    eprintln!("openrig: invalid --mcp address: {rest}");
                    None
                }
            };
        }
    }
    None
}

pub fn parse_cli_args_from(args: &[&str]) -> (Option<PathBuf>, bool, bool) {
    let mut project_path: Option<PathBuf> = None;
    let mut auto_save = false;
    let mut fullscreen = false;
    let mut i = 1;
    while i < args.len() {
        let arg = args[i];
        if arg == "--auto-save" {
            auto_save = true;
        } else if arg == "--fullscreen" {
            fullscreen = true;
        } else if arg == "--project" {
            // Explicit form: `--project <PATH>` (the documented #436 form).
            // A missing value is ignored so a stray flag never panics.
            if let Some(value) = args.get(i + 1) {
                project_path = Some(PathBuf::from(value));
                i += 1;
            }
        } else if !arg.starts_with('-') {
            project_path = Some(PathBuf::from(arg));
        }
        i += 1;
    }
    (project_path, auto_save, fullscreen)
}

/// Validate a project path resolved from `--project` / positional / env,
/// with a clear, path-naming error. `main.rs` uses this to fall back to the
/// launcher (no crash) when the path is bad.
pub fn validate_project_path(path: &Path) -> Result<(), String> {
    if !path.exists() {
        return Err(format!("project file not found: {}", path.display()));
    }
    if !path.is_file() {
        return Err(format!("project path is not a file: {}", path.display()));
    }
    Ok(())
}

#[cfg(test)]
#[path = "cli_tests.rs"]
mod tests;
