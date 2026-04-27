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

use std::path::PathBuf;

pub fn parse_cli_args_from(args: &[&str]) -> (Option<PathBuf>, bool, bool) {
    let mut project_path: Option<PathBuf> = None;
    let mut auto_save = false;
    let mut fullscreen = false;
    for arg in args.iter().skip(1) {
        if *arg == "--auto-save" {
            auto_save = true;
        } else if *arg == "--fullscreen" {
            fullscreen = true;
        } else if !arg.starts_with('-') {
            project_path = Some(PathBuf::from(arg));
        }
    }
    (project_path, auto_save, fullscreen)
}
