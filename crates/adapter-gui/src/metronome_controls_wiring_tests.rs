use super::*;

fn device(id: &str, name: &str) -> AudioDeviceDescriptor {
    AudioDeviceDescriptor {
        id: id.into(),
        name: name.into(),
        channels: 2,
    }
}

fn devices() -> Vec<AudioDeviceDescriptor> {
    vec![
        device("dev:scarlett", "Scarlett 2i2 USB"),
        device("dev:speakers", "MacBook Pro Speakers"),
        device("dev:teyun", "TEYUN Q-24"),
    ]
}

fn names(filtered: Vec<&AudioDeviceDescriptor>) -> Vec<&str> {
    filtered.iter().map(|d| d.name.as_str()).collect()
}

#[test]
fn an_empty_query_lists_every_device_in_order() {
    let all = devices();
    assert_eq!(
        names(filter_output_devices(&all, "")),
        vec!["Scarlett 2i2 USB", "MacBook Pro Speakers", "TEYUN Q-24"]
    );
    // A query of nothing but spaces is still "show me everything".
    assert_eq!(filter_output_devices(&all, "   ").len(), 3);
}

#[test]
fn the_query_matches_anywhere_in_the_name_whatever_the_case() {
    let all = devices();
    assert_eq!(
        names(filter_output_devices(&all, "teyun")),
        vec!["TEYUN Q-24"]
    );
    assert_eq!(
        names(filter_output_devices(&all, "SPEAK")),
        vec!["MacBook Pro Speakers"]
    );
}

#[test]
fn a_query_that_matches_nothing_lists_nothing() {
    // The select then renders its empty state instead of a stale list.
    assert!(filter_output_devices(&devices(), "focusrite 18i20").is_empty());
}

#[test]
fn the_query_reads_the_name_not_the_device_id() {
    // Device ids are opaque host strings; the user types what they see.
    assert!(filter_output_devices(&devices(), "dev:").is_empty());
}
