use super::*;

fn output(key: &str, label: &str) -> ProjectOutput {
    ProjectOutput {
        binding_id: String::new(),
        endpoint: String::new(),
        key: key.into(),
        label: label.into(),
        device_id: "dev:x".into(),
        channels: vec![0, 1],
        aliases: vec![],
    }
}

fn outputs() -> Vec<ProjectOutput> {
    vec![
        output("main\u{1f}Out 1-2", "Scarlett 2i2 · Out 1-2"),
        output("monitor\u{1f}Phones", "Headphones · Phones"),
        output("main\u{1f}Out 3-4", "Scarlett 2i2 · Out 3-4"),
    ]
}

fn labels(filtered: Vec<&ProjectOutput>) -> Vec<&str> {
    filtered.iter().map(|o| o.label.as_str()).collect()
}

#[test]
fn an_empty_query_lists_every_endpoint_in_order() {
    let all = outputs();
    assert_eq!(
        labels(filter_outputs(&all, "")),
        vec![
            "Scarlett 2i2 · Out 1-2",
            "Headphones · Phones",
            "Scarlett 2i2 · Out 3-4"
        ]
    );
    // A query of nothing but spaces is still "show me everything".
    assert_eq!(filter_outputs(&all, "   ").len(), 3);
}

#[test]
fn the_query_matches_anywhere_in_the_label_whatever_the_case() {
    let all = outputs();
    assert_eq!(
        labels(filter_outputs(&all, "phones")),
        vec!["Headphones · Phones"]
    );
    assert_eq!(
        labels(filter_outputs(&all, "OUT 3")),
        vec!["Scarlett 2i2 · Out 3-4"]
    );
}

#[test]
fn a_query_that_matches_nothing_lists_nothing() {
    // The select then renders its empty state instead of a stale list.
    assert!(filter_outputs(&outputs(), "focusrite 18i20").is_empty());
}

#[test]
fn the_query_reads_the_label_not_the_key() {
    // Keys are opaque; the user types what they see.
    assert!(filter_outputs(&outputs(), "\u{1f}").is_empty());
}

#[test]
fn refreshing_lists_the_projects_outputs_on_the_hosts_devices_and_caches_them() {
    let session = crate::chain_graph_fixtures_tests::session_with(vec![]);
    let cache = Rc::new(RefCell::new(outputs()));

    let refreshed = refresh_metronome_outputs(&session, &cache);

    let expected = output_endpoints(
        &crate::chain_graph_fixtures_tests::registry(),
        &crate::device_refresh_list::list_output_devices(),
    );
    assert_eq!(refreshed, expected);
    assert_eq!(
        *cache.borrow(),
        expected,
        "the cache holds what was returned"
    );
}
