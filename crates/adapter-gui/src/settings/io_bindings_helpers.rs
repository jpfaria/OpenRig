//! Responsibility: turns a bindings-section gesture into the command it stands for.

use crate::state::ProjectSession;
use application::command::{Command, IoBindingCommand};
use domain::io_binding::IoBinding;
use infra_filesystem::AppConfig;
use std::cell::RefCell;
use std::rc::Rc;

// ── Pure helpers (testable without AppWindow) ─────────────────────────────────

/// Build an `IoBindingCommand::CreateIoBinding` for a new binding.
pub(crate) fn build_create_command(binding: IoBinding) -> Command {
    Command::IoBinding(IoBindingCommand::CreateIoBinding { binding })
}

/// Convert a dispatcher reject `Err` into a display string for the UI.
/// Leaves `list` unchanged — the delete was rejected, so no mutation.
pub(crate) fn surface_delete_error(err: &anyhow::Error, _list: &mut Vec<IoBinding>) -> String {
    err.to_string()
}

// ── Internal helpers ──────────────────────────────────────────────────────────

pub(crate) fn dispatch_if_session(ps: &Rc<RefCell<Option<ProjectSession>>>, cmd: Command) {
    if let Some(session) = ps.borrow().as_ref() {
        let _ = session.dispatcher.dispatch(cmd);
    }
}

/// Name for a new binding: the typed name, or a sequential default ("I/O N").
pub(crate) fn binding_display_name(name: &str, bindings: &[IoBinding]) -> String {
    let trimmed = name.trim();
    if !trimmed.is_empty() {
        return trimmed.to_string();
    }
    format!("I/O {}", bindings.len() + 1)
}

/// Refresh the GUI's `AppConfig` snapshot FROM the registry the dispatcher owns
/// (#127).
///
/// The mirror used to run the other way — snapshot INTO the registry — so a
/// binding created from MCP/gRPC landed in the registry and was then wiped by
/// the next click on this screen: the command reported success and a GUI code
/// path silently undid it. Inverting it makes the dispatcher's registry the
/// single source of truth, and the other GUI readers of `AppConfig.io_bindings`
/// (chain CRUD, device refresh, the audio section) see those edits too.
pub(crate) fn sync_snapshot_from_registry(
    ps: &Rc<RefCell<Option<ProjectSession>>>,
    cfg: &Rc<RefCell<AppConfig>>,
) {
    let registry = ps
        .borrow()
        .as_ref()
        .map(|session| session.io_bindings.borrow().clone());
    if let Some(bindings) = registry {
        cfg.borrow_mut().io_bindings = bindings;
    }
}

/// #716 (AUDIO-CRITICAL), as a Command since #127: install the edited registry
/// into the live runtime so an ALREADY-RUNNING chain re-resolves its device
/// endpoints against the latest edit instead of waiting for the next cold
/// start. The GUI used to call the controller directly, which left MCP/gRPC
/// with no way to reach the live registry at all.
pub(crate) fn push_bindings_to_runtime(ps: &Rc<RefCell<Option<ProjectSession>>>) {
    dispatch_if_session(ps, Command::IoBinding(IoBindingCommand::SetIoBindings));
}

pub(crate) fn delete_reject_message(ps: &Rc<RefCell<Option<ProjectSession>>>, id: &str) -> String {
    let cmd = Command::IoBinding(IoBindingCommand::DeleteIoBinding { id: id.to_string() });
    if let Some(session) = ps.borrow().as_ref() {
        match session.dispatcher.dispatch(cmd) {
            Ok(_) => String::new(),
            Err(e) => {
                let mut dummy: Vec<IoBinding> = Vec::new();
                surface_delete_error(&e, &mut dummy)
            }
        }
    } else {
        String::new()
    }
}

/// Generate a slug-style id from the binding name + a small hash.
pub(crate) fn make_id(name: &str) -> String {
    use std::collections::hash_map::DefaultHasher;
    use std::hash::{Hash, Hasher};
    use std::time::SystemTime;

    let mut h = DefaultHasher::new();
    name.hash(&mut h);
    SystemTime::now()
        .duration_since(SystemTime::UNIX_EPOCH)
        .unwrap_or_default()
        .subsec_nanos()
        .hash(&mut h);

    let slug: String = name
        .chars()
        .filter(|c| c.is_alphanumeric() || *c == ' ' || *c == '-')
        .map(|c| {
            if c == ' ' {
                '-'
            } else {
                c.to_ascii_lowercase()
            }
        })
        .take(24)
        .collect();

    format!("{slug}-{:x}", h.finish() & 0xffff)
}
