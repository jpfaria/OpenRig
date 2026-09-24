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

// #978, ASIO on cpal 0.18: a Device is only the driver's name and cached
// metadata, and every output stream of one driver has to be built from clones
// of ONE handle, or each stream clears the shared buffer and the last one built
// erases the others. And while a driver is running, enumeration stops at the
// first other driver's name, so a walk can come back short.

#[test]
fn a_merging_lookup_hands_out_the_same_handle_without_walking_again() {
    let table = Remembered::<Fake>::new();
    let walks = Cell::new(0);
    let walk = || {
        walks.set(walks.get() + 1);
        Ok(vec![("asio:quantum".to_string(), ("asio:quantum", 1))])
    };
    let first = table.find_merging("asio:quantum", walk).unwrap();
    let again = table.find_merging("asio:quantum", walk).unwrap();
    assert_eq!(first, again);
    assert_eq!(walks.get(), 1);
}

#[test]
fn a_short_walk_does_not_forget_drivers_found_before() {
    let table = Remembered::<Fake>::new();
    table
        .find_merging("asio:quantum", || {
            Ok(vec![
                ("asio:quantum".to_string(), ("asio:quantum", 1)),
                ("asio:focusrite".to_string(), ("asio:focusrite", 1)),
            ])
        })
        .unwrap();
    // A later walk (Quantum running) stops before Focusrite and adds one new
    // driver: both earlier ones must still be found.
    let fresh = table
        .find_merging("asio:behringer", || {
            Ok(vec![("asio:behringer".to_string(), ("asio:behringer", 1))])
        })
        .unwrap();
    assert_eq!(fresh, Some(("asio:behringer", 1)));
    let walked_again = Cell::new(false);
    let focusrite = table
        .find_merging("asio:focusrite", || {
            walked_again.set(true);
            Ok(vec![])
        })
        .unwrap();
    assert_eq!(focusrite, Some(("asio:focusrite", 1)));
    assert!(!walked_again.get(), "a remembered driver is not walked for");
}
