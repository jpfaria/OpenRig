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

/// Default MCP bind address: bare `--mcp` and the config master switch
/// (#712) both resolve to this. Single source of truth.
pub const DEFAULT_MCP_ADDR: &str = "127.0.0.1:4123";

/// Parse the opt-in MCP server flag.
///
/// * `--mcp` → default [`DEFAULT_MCP_ADDR`]
/// * `--mcp=ADDR` → that socket address (invalid value → `None`, logged)
/// * absent → `None` (server not started; zero overhead)
pub fn parse_mcp_addr(args: &[&str]) -> Option<SocketAddr> {
    for arg in args.iter().skip(1) {
        if *arg == "--mcp" {
            return Some(DEFAULT_MCP_ADDR.parse().expect("valid default mcp addr"));
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

/// How the opt-in `--midi` flag was given. Resolving `Default` to the
/// per-OS `midi-map.yaml` path is the caller's job (keeps this parser pure).
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum MidiMapArg {
    /// `--midi` → use the per-OS default `midi-map.yaml`.
    Default,
    /// `--midi=PATH` → use this explicit mapping file.
    Path(PathBuf),
}

/// Parse the opt-in MIDI controller adapter flag (issue #22).
///
/// * `--midi` → [`MidiMapArg::Default`]
/// * `--midi=PATH` → [`MidiMapArg::Path`]
/// * absent → `None` (adapter not started; zero overhead)
pub fn parse_midi_map(args: &[&str]) -> Option<MidiMapArg> {
    for arg in args.iter().skip(1) {
        if *arg == "--midi" {
            return Some(MidiMapArg::Default);
        }
        if let Some(rest) = arg.strip_prefix("--midi=") {
            return Some(MidiMapArg::Path(PathBuf::from(rest)));
        }
    }
    None
}

/// #712: fold the per-machine config master switch into the CLI result
/// for the MCP server. A present CLI flag (`Some`) is a dev override and
/// always wins; otherwise `config_enabled` decides, binding the default
/// address. Both off → `None` (server stays down).
pub fn resolve_mcp_addr(cli: Option<SocketAddr>, config_enabled: bool) -> Option<SocketAddr> {
    cli.or_else(|| {
        config_enabled.then(|| DEFAULT_MCP_ADDR.parse().expect("valid default mcp addr"))
    })
}

/// #712: same fold for the MIDI adapter. CLI flag wins; otherwise the
/// config switch enables it with the default resolved map.
pub fn resolve_midi_map(cli: Option<MidiMapArg>, config_enabled: bool) -> Option<MidiMapArg> {
    cli.or_else(|| config_enabled.then_some(MidiMapArg::Default))
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
