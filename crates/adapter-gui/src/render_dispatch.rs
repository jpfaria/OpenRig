//! Classify a raw argv into either GUI launch or an offline render run.
//!
//! Lives here (in `adapter-gui`) rather than in `adapter-render` because
//! the dispatch decision is the GUI binary's job — only it knows about
//! all the live-rig flags (`--mcp`, `--midi`, positional project path)
//! that `--render` must be mutually exclusive with.
//!
//! Pure function — no IO, no Slint, no engine. Used by `main.rs` to
//! decide whether to bring up the window or hand off to
//! `adapter_render::render()`.

use adapter_render::cli::{parse_render_args, RenderArgs, RenderArgsError};

/// What the binary should do after parsing argv.
#[derive(Debug)]
pub enum LaunchMode {
    /// Bring up the Slint window (default).
    Gui,
    /// Hand off to `adapter_render::render()` and exit.
    Render(RenderArgs),
}

/// Errors raised by [`classify_launch`].
#[derive(Debug)]
pub enum LaunchError {
    /// `--render` was combined with a flag/argument that requires the live
    /// rig (e.g. `--mcp`, `--midi`, a positional project path).
    MutuallyExclusive(String),
    /// `--render` was given but the render args themselves are invalid.
    RenderArgs(RenderArgsError),
}

impl std::fmt::Display for LaunchError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::MutuallyExclusive(flag) => {
                write!(f, "--render is mutually exclusive with {flag}")
            }
            Self::RenderArgs(e) => write!(f, "invalid render args: {e}"),
        }
    }
}

impl std::error::Error for LaunchError {}

/// Decide whether to launch the GUI or hand off to offline render.
pub fn classify_launch(argv: &[String]) -> Result<LaunchMode, LaunchError> {
    if !has_render_flag(argv) {
        return Ok(LaunchMode::Gui);
    }

    // `--render` is set — enforce mutual exclusion with live-rig flags.
    if has_flag_or_eq(argv, "--mcp") {
        return Err(LaunchError::MutuallyExclusive("--mcp".into()));
    }
    if has_flag_or_eq(argv, "--midi") {
        return Err(LaunchError::MutuallyExclusive("--midi".into()));
    }
    if has_positional_argument(argv) {
        return Err(LaunchError::MutuallyExclusive(
            "<positional project path>".into(),
        ));
    }

    parse_render_args(argv)
        .map(LaunchMode::Render)
        .map_err(LaunchError::RenderArgs)
}

fn has_render_flag(argv: &[String]) -> bool {
    argv.iter().any(|a| a == "--render")
}

/// True iff `argv` contains exactly `flag` (boolean form) or `flag=...`
/// (value form). Both forms count as "the flag is set".
fn has_flag_or_eq(argv: &[String], flag: &str) -> bool {
    let prefix = format!("{flag}=");
    argv.iter().any(|a| a == flag || a.starts_with(&prefix))
}

/// True iff `argv` contains a positional argument (anything that doesn't
/// start with `--` and isn't the binary name at index 0, and isn't the
/// value of a known value-bearing flag).
///
/// Conservative: any non-flag, non-value slot is treated as a positional.
fn has_positional_argument(argv: &[String]) -> bool {
    let value_flags: &[&str] = &[
        "--project",
        "--input",
        "--output",
        "--chain",
        "--sample-rate",
        "--block-size",
        "--bit-depth",
        "--tail-ms",
    ];
    let mut i = 1; // skip binary name
    while i < argv.len() {
        let a = &argv[i];
        if a.starts_with("--") {
            // Value-bearing flag with separate value → skip both
            if value_flags.iter().any(|vf| a == vf) {
                i += 2;
                continue;
            }
            // Boolean or `--flag=value` → skip one
            i += 1;
            continue;
        }
        // Naked argument outside a flag context → positional.
        return true;
    }
    false
}
