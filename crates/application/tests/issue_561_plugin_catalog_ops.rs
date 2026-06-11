//! Issue #561 (expanded scope): list / get / load / unload individual
//! plugins through the catalog without restarting.
//!
//! The reload Command (already in the bus) covers the whole-disk
//! rescan flow. The agent-driven tone-builder flow also needs the
//! finer-grained operations:
//!
//! * `Query::ListPluginCatalog` — enumerate every plugin (native +
//!   disk) the running process knows about. Each entry carries id,
//!   display_name, brand, block_type, backend ("native" / "disk").
//! * `Query::GetPlugin { id }` — single entry by id, `null` when not
//!   present. Lets the agent "find" or "get" without paging the full
//!   list every time.
//! * `Command::LoadPlugin { id }` — bring a single plugin into the
//!   registry. Re-scans the known plugin roots and adds the one whose
//!   manifest id matches. Errors cleanly when no package on disk
//!   carries that id.
//! * `Command::UnloadPlugin { id }` — remove a single disk plugin
//!   from the in-memory registry. Refuses natives (compiled-in;
//!   cannot be dropped without restarting the process).
//!
//! Per the architecture LAWs (`CLAUDE.md`):
//! - Each Command is a variant on `application::command::Command` so
//!   MCP/gRPC inherit the operation through the schema-derived tool
//!   surface (`parity_guard_every_command_variant_is_a_tool`).
//! - Each Query is a `pub` function in `application::query` so
//!   adapters can call it directly OR through the bridge for cross-
//!   thread reads (mirrors `list_project_presets`).

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

/// Bring at least one native into the catalog so the listing/get
/// tests have stable, non-empty fixtures without depending on disk.
fn seed_natives_and_reload(dispatcher: &LocalDispatcher) {
    engine::native_registry::register_all_natives();
    let _ = dispatcher
        .dispatch(Command::ReloadPluginCatalog)
        .expect("reload to seed natives");
    // #693: the rescan runs on its own task — wait for the completion.
    wait_async(dispatcher, |e| {
        matches!(e, Event::PluginCatalogReloaded { .. })
    });
}

/// #693 helper: poll async completions until `pred` matches (2s cap);
/// returns every event drained along the way.
fn wait_async(
    dispatcher: &LocalDispatcher,
    pred: impl Fn(&Event) -> bool,
) -> Vec<Event> {
    let deadline = std::time::Instant::now() + std::time::Duration::from_secs(2);
    let mut all = Vec::new();
    while std::time::Instant::now() < deadline {
        all.extend(dispatcher.poll_async_results());
        if all.iter().any(&pred) {
            break;
        }
        std::thread::sleep(std::time::Duration::from_millis(10));
    }
    all
}

/// Extract the first `"id": "<x>"` value found in a JSON listing.
/// Crude but sufficient for these tests — we don't want to pull a
/// json crate into the integration test just to read one field.
fn first_id_in_listing(json: &str) -> String {
    let needle = "\"id\": \"";
    let start = json.find(needle).expect("at least one id");
    let after = &json[start + needle.len()..];
    let end = after.find('"').expect("closing quote");
    after[..end].to_string()
}

#[test]
fn list_plugin_catalog_returns_an_array_with_at_least_one_native() {
    let project = empty_project_rc();
    let dispatcher = LocalDispatcher::new(Rc::clone(&project));
    seed_natives_and_reload(&dispatcher);

    let json = application::query::list_plugin_catalog();
    assert!(
        json.contains("\"plugins\""),
        "listing must wrap entries under a `plugins` key, got: {json}"
    );
    assert!(
        json.contains("\"backend\": \"native\""),
        "at least one native must surface in the listing, got: {json}"
    );
}

#[test]
fn get_plugin_returns_entry_when_id_is_known() {
    let project = empty_project_rc();
    let dispatcher = LocalDispatcher::new(Rc::clone(&project));
    seed_natives_and_reload(&dispatcher);

    let listing = application::query::list_plugin_catalog();
    let id = first_id_in_listing(&listing);

    let json = application::query::get_plugin(&id);
    assert!(
        json.contains(&format!("\"id\": \"{id}\"")),
        "get_plugin must return the entry for {id}, got: {json}"
    );
}

#[test]
fn get_plugin_returns_null_when_id_is_unknown() {
    let json = application::query::get_plugin("definitely_not_a_real_plugin_id_561");
    assert!(
        json.contains("null"),
        "get_plugin must surface a null when the id is unknown, got: {json}"
    );
}

#[test]
fn unload_plugin_command_errors_when_id_is_unknown() {
    let project = empty_project_rc();
    let dispatcher = LocalDispatcher::new(Rc::clone(&project));

    let err = dispatcher
        .dispatch(Command::UnloadPlugin {
            id: "definitely_not_a_real_plugin_id_561".into(),
        })
        .expect_err("unloading an unknown id must error cleanly");
    let msg = err.to_string();
    assert!(
        msg.contains("not found") || msg.contains("unknown"),
        "expected a 'not found / unknown' message, got: {msg}"
    );
}

#[test]
fn unload_plugin_command_refuses_to_drop_a_native() {
    let project = empty_project_rc();
    let dispatcher = LocalDispatcher::new(Rc::clone(&project));
    seed_natives_and_reload(&dispatcher);

    let listing = application::query::list_plugin_catalog();
    let native_id = first_id_in_listing(&listing);

    let err = dispatcher
        .dispatch(Command::UnloadPlugin {
            id: native_id.clone(),
        })
        .expect_err("unloading a native must error");
    let msg = err.to_string();
    assert!(
        msg.contains("native"),
        "expected the error to mention 'native' (compiled-in, cannot unload), got: {msg}"
    );
}

#[test]
fn load_plugin_command_errors_when_id_is_unknown_on_disk() {
    let project = empty_project_rc();
    let dispatcher = LocalDispatcher::new(Rc::clone(&project));

    // #693: the root scan runs on its own task — the failure surfaces
    // as Event::PluginLoadFailed via the async-completion poll.
    let events = dispatcher
        .dispatch(Command::LoadPlugin {
            id: "definitely_not_a_real_plugin_id_561".into(),
        })
        .expect("dispatch only enqueues the scan");
    assert!(events.is_empty(), "scan must run off-thread, got {events:?}");
    let done = wait_async(&dispatcher, |e| {
        matches!(e, Event::PluginLoadFailed { .. })
    });
    let msg = done
        .iter()
        .find_map(|e| match e {
            Event::PluginLoadFailed { reason, .. } => Some(reason.clone()),
            _ => None,
        })
        .expect("expected PluginLoadFailed via poll");
    assert!(
        msg.contains("not found") || msg.contains("unknown"),
        "expected a 'not found / unknown' reason, got: {msg}"
    );
}

#[test]
fn load_plugin_command_with_known_native_id_emits_plugin_loaded_event() {
    // Natives are already in the registry (via register_all_natives
    // + reload); LoadPlugin {id} on an already-loaded id must be a
    // no-op that still emits the event so the caller can confirm.
    let project = empty_project_rc();
    let dispatcher = LocalDispatcher::new(Rc::clone(&project));
    seed_natives_and_reload(&dispatcher);

    let listing = application::query::list_plugin_catalog();
    let id = first_id_in_listing(&listing);

    let events = dispatcher
        .dispatch(Command::LoadPlugin { id: id.clone() })
        .expect("load known id");
    assert!(events.is_empty(), "scan must run off-thread, got {events:?}");
    // #693: the confirmation arrives via the async-completion poll.
    let done = wait_async(&dispatcher, |e| {
        matches!(e, Event::PluginLoaded { id: ev_id } if ev_id == &id)
    });
    assert!(
        done.iter().any(|e| matches!(
            e,
            Event::PluginLoaded { id: ev_id } if ev_id == &id
        )),
        "expected Event::PluginLoaded {{ id={id} }} via poll, got: {done:?}"
    );
}
