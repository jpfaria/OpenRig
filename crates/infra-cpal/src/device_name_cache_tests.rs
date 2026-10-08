//! #1081 — the periodic device scan must not ask a device its name again.
//! cpal's `description()` opens two AudioUnits per device, and each one starts
//! and stops an IOProc on the default output device (the HD 8) inside
//! OpenRig's own IO context — 15 times every 10 s while the rig plays.

use std::cell::Cell;

use anyhow::anyhow;

use super::Names;

#[test]
fn a_device_is_asked_its_name_once() {
    let names = Names::new();
    let asked = Cell::new(0);
    let ask = || {
        asked.set(asked.get() + 1);
        Ok("Quantum HD 8".to_string())
    };
    assert_eq!(names.name("hd8", ask).unwrap(), "Quantum HD 8");
    assert_eq!(names.name("hd8", ask).unwrap(), "Quantum HD 8");
    assert_eq!(names.name("hd8", ask).unwrap(), "Quantum HD 8");
    assert_eq!(asked.get(), 1);
}

#[test]
fn each_device_is_asked_for_its_own_name() {
    let names = Names::new();
    assert_eq!(
        names.name("hd8", || Ok("Quantum HD 8".into())).unwrap(),
        "Quantum HD 8"
    );
    assert_eq!(
        names.name("bh", || Ok("BlackHole 2ch".into())).unwrap(),
        "BlackHole 2ch"
    );
    assert_eq!(
        names.name("hd8", || Ok("wrong".into())).unwrap(),
        "Quantum HD 8"
    );
}

#[test]
fn a_device_list_change_asks_again() {
    let names = Names::new();
    names.name("hd8", || Ok("Quantum HD 8".into())).unwrap();
    names.forget();
    assert_eq!(
        names.name("hd8", || Ok("Renamed".into())).unwrap(),
        "Renamed"
    );
}

#[test]
fn a_name_that_could_not_be_read_is_asked_again() {
    let names = Names::new();
    assert!(names.name("hd8", || Err(anyhow!("busy"))).is_err());
    assert_eq!(
        names.name("hd8", || Ok("Quantum HD 8".into())).unwrap(),
        "Quantum HD 8"
    );
}
