//! Responsibility: normalises a freshly read project.
//! Load-time normalization of a freshly-read project, before the session and
//! the runtime see it. Same passes the dispatcher's `LoadProject` handler
//! applies, so a GUI open and an MCP/gRPC open agree on the resulting state.
//!
//! - #606: disable blocks whose model is not installed on this machine — the
//!   chain plays with the pedal visibly off instead of silently faulting.
//! - #833: disable a chain whose physical input (device + channel) is already
//!   captured by an earlier enabled chain — one guitar must never be
//!   double-processed by two runtimes.

use infra_filesystem::IoBinding;
use project::project::Project;

/// Apply every load-time pass to `project`, logging what each one changed.
pub fn normalize_loaded_project(project: &mut Project, registry: &[IoBinding]) {
    let disabled = project::project_disable_unavailable::disable_unavailable_blocks(project);
    if !disabled.is_empty() {
        log::warn!(
            "disabled {} block(s) with unavailable models on load: {:?}",
            disabled.len(),
            disabled.iter().map(|b| &b.0).collect::<Vec<_>>()
        );
    }

    let contending = engine::runtime_endpoints::disable_conflicting_chains(project, registry);
    if !contending.is_empty() {
        log::warn!(
            "disabled {} chain(s) whose input is already captured by an earlier \
             enabled chain: {:?}",
            contending.len(),
            contending.iter().map(|c| &c.0).collect::<Vec<_>>()
        );
    }
}
