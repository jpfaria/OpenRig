use super::filter_di_sources;

#[test]
fn filter_di_sources_empty_query_returns_all() {
    let sources = vec![
        "Clean DI".to_string(),
        "Crunch DI".to_string(),
        "Lead DI".to_string(),
    ];
    let got: Vec<&String> = filter_di_sources(&sources, "");
    assert_eq!(got, vec![&sources[0], &sources[1], &sources[2]]);

    // Whitespace-only query is treated as empty.
    let got_ws: Vec<&String> = filter_di_sources(&sources, "   ");
    assert_eq!(got_ws, vec![&sources[0], &sources[1], &sources[2]]);
}

#[test]
fn filter_di_sources_matches_case_insensitive_substring() {
    let sources = vec![
        "Clean DI".to_string(),
        "Crunch DI".to_string(),
        "Lead Tone".to_string(),
    ];
    // "di" matches the two "DI" sources, in original order.
    let got: Vec<&String> = filter_di_sources(&sources, "di");
    assert_eq!(got, vec![&sources[0], &sources[1]]);

    // Uppercase query still matches.
    let got_upper: Vec<&String> = filter_di_sources(&sources, "CRUNCH");
    assert_eq!(got_upper, vec![&sources[1]]);
}

#[test]
fn filter_di_sources_no_match_returns_empty() {
    let sources = vec!["Clean DI".to_string(), "Crunch DI".to_string()];
    let got: Vec<&String> = filter_di_sources(&sources, "zzz");
    assert!(got.is_empty());
}
