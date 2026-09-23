//! Tests for the remembered device lookup (#967), without a sound card: a
//! "device" is a `(id, generation)` pair and the walk is counted.

use std::cell::Cell;

use super::Remembered;

type Fake = (&'static str, u32);

fn still_names(device: &Fake, id: &str) -> bool {
    device.0 == id && device.1 == 1
}

#[test]
fn a_repeat_lookup_does_not_walk_the_devices_again() {
    let table = Remembered::<Fake>::new();
    let walks = Cell::new(0);
    let walk = || {
        walks.set(walks.get() + 1);
        Ok(vec![
            ("hd8".to_string(), ("hd8", 1)),
            ("teyun".to_string(), ("teyun", 1)),
        ])
    };
    assert_eq!(
        table.find("hd8", still_names, walk).unwrap(),
        Some(("hd8", 1))
    );
    assert_eq!(
        table.find("teyun", still_names, walk).unwrap(),
        Some(("teyun", 1))
    );
    assert_eq!(
        table.find("hd8", still_names, walk).unwrap(),
        Some(("hd8", 1))
    );
    assert_eq!(walks.get(), 1, "one walk serves every device it passed");
}

#[test]
fn a_handle_that_no_longer_names_its_id_is_looked_up_again() {
    let table = Remembered::<Fake>::new();
    let walks = Cell::new(0);
    table
        .find("hd8", still_names, || {
            Ok(vec![("hd8".to_string(), ("hd8", 2))])
        })
        .unwrap();
    // The remembered handle is stale (generation 2 fails the check): walk.
    let found = table
        .find("hd8", still_names, || {
            walks.set(walks.get() + 1);
            Ok(vec![("hd8".to_string(), ("hd8", 1))])
        })
        .unwrap();
    assert_eq!((found, walks.get()), (Some(("hd8", 1)), 1));
}

#[test]
fn forgetting_forces_a_new_walk() {
    let table = Remembered::<Fake>::new();
    let walks = Cell::new(0);
    let walk = || {
        walks.set(walks.get() + 1);
        Ok(vec![("hd8".to_string(), ("hd8", 1))])
    };
    table.find("hd8", still_names, walk).unwrap();
    table.forget();
    table.find("hd8", still_names, walk).unwrap();
    assert_eq!(walks.get(), 2);
}

#[test]
fn an_unknown_id_is_none_after_one_walk() {
    let table = Remembered::<Fake>::new();
    let found = table
        .find("gone", still_names, || {
            Ok(vec![("hd8".to_string(), ("hd8", 1))])
        })
        .unwrap();
    assert_eq!(found, None);
}
