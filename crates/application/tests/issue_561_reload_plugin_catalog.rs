//! Issue #561: hot-reload the plugin catalog without restarting OpenRig.
//!
//! Today the catalog is loaded once at boot
//! (`plugin_loader::registry::init_many`) in `crates/adapter-gui/src/desktop_app.rs`.
//! After importing a new NAM/IR pack into `OpenRig-plugins` the new model is
//! unreachable until the user quits and reopens the app. This RED test pins
//! the command + event contract that closes that gap:
//!
//! * A new `Command::ReloadPluginCatalog` (no payload) exists.
//! * Dispatching it succeeds and emits an `Event::PluginCatalogReloaded`
//!   that carries `native_count`, `disk_count`, and `total_count` such
//!   that `total_count == native_count + disk_count`.
//! * `native_count >= 1` — natives are compiled in, so any boot state
//!   already has at least one registered model. This holds even when the
//!   plugins dir on disk is empty.
//!
//! Per the architecture LAWs in `CLAUDE.md`, the variant lives in
//! `application::command`, the handler in the `LocalDispatcher`, and MCP
//! parity is auto-derived from the `Command` schema — so a follow-up MCP
//! test is not needed here.

use std::cell::RefCell;
use std::rc::Rc;

use project::project::Project;

use application::command::Command;
use application::dispatcher::CommandDispatcher;
use application::event::Event;
use application::local_dispatcher::LocalDispatcher;

fn empty_project_rc() -> Rc<RefCell<Project>> {
    Rc::new(RefCell::new(Project {
        name: Some("test".into()),
        device_settings: Vec::new(),
        chains: Vec::new(),
        midi: None,
    }))
}

#[test]
fn reload_plugin_catalog_emits_plugin_catalog_reloaded_with_counts() {
    // Populate the native side of the catalog so `native_count >= 1` is a
    // meaningful invariant — the contract is "natives are compiled in, so
    // any boot state has at least one".
    engine::native_registry::register_all_natives();

    let project = empty_project_rc();
    let dispatcher = LocalDispatcher::new(Rc::clone(&project));

    let mut events = dispatcher
        .dispatch(Command::ReloadPluginCatalog)
        .expect("dispatch ReloadPluginCatalog");
    // #693: the rescan runs on its own task — the completion event
    // arrives via poll_async_results (the frontend tick's job).
    let deadline = std::time::Instant::now() + std::time::Duration::from_secs(2);
    while events.is_empty() && std::time::Instant::now() < deadline {
        std::thread::sleep(std::time::Duration::from_millis(10));
        events = dispatcher.poll_async_results();
    }

    let reloaded = events
        .iter()
        .find_map(|e| match e {
            Event::PluginCatalogReloaded {
                native_count,
                disk_count,
                total_count,
            } => Some((*native_count, *disk_count, *total_count)),
            _ => None,
        })
        .unwrap_or_else(|| panic!("expected Event::PluginCatalogReloaded, got: {events:?}"));

    let (native, disk, total) = reloaded;
    assert_eq!(
        total,
        native + disk,
        "total_count must equal native_count + disk_count (native={native}, disk={disk}, total={total})"
    );
    assert!(
        native >= 1,
        "native_count must be >= 1 — natives are compiled in (got {native})"
    );
}
