//! Issue #561 (expanded scope, search): `Query::FindPlugins` — text
//! search across the in-memory plugin catalog (id / display_name /
//! brand, case-insensitive substring). Pairs with `ListPluginCatalog`
//! / `GetPlugin` so the agent can "search" the catalog without paging
//! the full list every time.

use std::cell::RefCell;
use std::rc::Rc;

use project::project::Project;

use application::command::Command;
use application::dispatcher::CommandDispatcher;
use application::local_dispatcher::LocalDispatcher;

fn empty_project_rc() -> Rc<RefCell<Project>> {
    Rc::new(RefCell::new(Project {
        name: Some("test".into()),
        device_settings: Vec::new(),
        chains: Vec::new(),
        midi: None,
    }))
}

fn seed_natives_and_reload(dispatcher: &LocalDispatcher) {
    engine::native_registry::register_all_natives();
    let _ = dispatcher
        .dispatch(Command::ReloadPluginCatalog)
        .expect("reload to seed natives");
    // #693: the rescan runs on its own task — wait for the completion
    // event (poll_async_results is the frontend tick's job).
    let deadline = std::time::Instant::now() + std::time::Duration::from_secs(2);
    while dispatcher.poll_async_results().is_empty() && std::time::Instant::now() < deadline {
        std::thread::sleep(std::time::Duration::from_millis(10));
    }
}

fn first_id_in_listing(json: &str) -> String {
    let needle = "\"id\": \"";
    let start = json.find(needle).expect("at least one id");
    let after = &json[start + needle.len()..];
    let end = after.find('"').expect("closing quote");
    after[..end].to_string()
}

#[test]
fn find_plugins_with_empty_query_returns_all_entries() {
    let project = empty_project_rc();
    let dispatcher = LocalDispatcher::new(Rc::clone(&project));
    seed_natives_and_reload(&dispatcher);

    let all = application::query::list_plugin_catalog();
    let found = application::query::find_plugins("");

    assert_eq!(
        all, found,
        "empty query must behave the same as list_plugin_catalog"
    );
}

#[test]
fn find_plugins_matches_native_id_substring_case_insensitive() {
    let project = empty_project_rc();
    let dispatcher = LocalDispatcher::new(Rc::clone(&project));
    seed_natives_and_reload(&dispatcher);

    let listing = application::query::list_plugin_catalog();
    let id = first_id_in_listing(&listing);
    // Slice the first 3 chars of an id we know exists — guaranteed
    // to match at least that one entry. Bumping to upper-case proves
    // the matcher is case-insensitive.
    let needle: String = id.chars().take(3).collect::<String>().to_uppercase();

    let json = application::query::find_plugins(&needle);
    assert!(
        json.contains(&format!("\"id\": \"{id}\"")),
        "find_plugins({needle:?}) must include {id}, got: {json}"
    );
}

#[test]
fn find_plugins_with_no_match_returns_empty_array() {
    let project = empty_project_rc();
    let dispatcher = LocalDispatcher::new(Rc::clone(&project));
    seed_natives_and_reload(&dispatcher);

    let json = application::query::find_plugins("definitely_not_a_real_substring_561_xyz");
    assert!(
        json.contains("\"plugins\": []"),
        "no-match query must emit an empty plugins array, got: {json}"
    );
}
