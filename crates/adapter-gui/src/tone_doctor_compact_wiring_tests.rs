use super::filter_genres;

#[test]
fn filter_is_case_insensitive_substring_and_keeps_order() {
    let genres = ["alternative-rock", "blues-rock", "grunge", "heavy-metal"];
    assert_eq!(
        filter_genres(&genres, "rock"),
        vec!["alternative-rock", "blues-rock"]
    );
    assert_eq!(filter_genres(&genres, "METAL"), vec!["heavy-metal"]);
}

#[test]
fn blank_query_returns_all() {
    let genres = ["grunge", "blues-rock"];
    assert_eq!(filter_genres(&genres, "  "), vec!["grunge", "blues-rock"]);
    assert!(filter_genres(&genres, "zzz").is_empty());
}
