//! The spectrum filter lists one checkbox per output the rows read, every one
//! checked until the user unchecks it, and remembers the unchecked ones
//! across session rebuilds.

use super::SpectrumFilter;

fn names(list: &[&str]) -> Vec<String> {
    list.iter().map(|s| s.to_string()).collect()
}

#[test]
fn every_output_is_listed_and_checked_until_the_user_unchecks_it() {
    let filter = SpectrumFilter::default();

    let items = filter.items(&names(&["A", "B"]));

    let listed: Vec<(i32, String, bool, bool)> = items
        .iter()
        .map(|i| (i.index, i.label.to_string(), i.selected, i.available))
        .collect();
    assert_eq!(
        listed,
        vec![
            (0, "A".to_string(), true, true),
            (1, "B".to_string(), true, true),
        ]
    );
    assert!(filter.shows("A") && filter.shows("B"));
}

#[test]
fn unchecking_an_output_hides_it() {
    let mut filter = SpectrumFilter::default();

    filter.set_shown("A", false);

    assert!(!filter.shows("A"));
    assert!(filter.shows("B"));
    assert!(!filter.items(&names(&["A", "B"]))[0].selected);
}

#[test]
fn checking_it_again_shows_it() {
    let mut filter = SpectrumFilter::default();
    filter.set_shown("A", false);

    filter.set_shown("A", true);

    assert!(filter.shows("A"));
}

#[test]
fn an_unchecked_output_stays_unchecked_when_the_rows_are_rebuilt() {
    let mut filter = SpectrumFilter::default();
    filter.items(&names(&["A", "B"]));
    filter.set_shown("A", false);

    let items = filter.items(&names(&["C", "A", "B"]));

    let selected: Vec<bool> = items.iter().map(|i| i.selected).collect();
    assert_eq!(selected, vec![true, false, true]);
}
