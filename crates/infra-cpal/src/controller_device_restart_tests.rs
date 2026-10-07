//! #1081 — which streams a device restart closes: every chain that reads or
//! plays on the device whose input broke, never a chain on another device;
//! and the device a stepped runtime reads, never another input of its chain.

use domain::ids::ChainId;

use super::{chains_on_devices, stepped_input_devices};

fn id(chain: &str) -> ChainId {
    ChainId(chain.into())
}

fn devices(list: &[&str]) -> Vec<String> {
    list.iter().map(|d| d.to_string()).collect()
}

#[test]
fn every_chain_that_reads_or_plays_on_the_device_is_restarted() {
    let live = vec![
        (id("rig:violao"), devices(&["hd8", "hd8"])),
        (id("rig:guitar"), devices(&["hd8", "hd8"])),
        (id("rig:keys"), devices(&["usb-mic", "hd8"])),
        (id("rig:bass"), devices(&["hd8", "headphones"])),
    ];
    assert_eq!(
        chains_on_devices(&devices(&["hd8"]), &live),
        vec![
            id("rig:bass"),
            id("rig:guitar"),
            id("rig:keys"),
            id("rig:violao")
        ]
    );
}

#[test]
fn a_chain_on_another_device_is_left_alone() {
    let live = vec![
        (id("rig:violao"), devices(&["hd8", "hd8"])),
        (id("rig:other"), devices(&["usb-mic", "headphones"])),
    ];
    assert_eq!(
        chains_on_devices(&devices(&["hd8"]), &live),
        vec![id("rig:violao")]
    );
}

#[test]
fn no_device_restarts_no_chain() {
    let live = vec![(id("rig:violao"), devices(&["hd8"]))];
    assert!(chains_on_devices(&[], &live).is_empty());
}

#[test]
fn the_stepped_runtime_names_the_device_it_reads() {
    // Entry order: two entries on the HD 8 share its stream (cpal input 0),
    // the USB mic is the next device (cpal input 1).
    let inputs = devices(&["hd8", "hd8", "usb-mic"]);
    assert_eq!(stepped_input_devices(&inputs, &[0]), devices(&["hd8"]));
    assert_eq!(stepped_input_devices(&inputs, &[1]), devices(&["usb-mic"]));
}

#[test]
fn two_stepped_runtimes_on_one_device_name_it_once() {
    let inputs = devices(&["hd8", "hd8"]);
    assert_eq!(stepped_input_devices(&inputs, &[0, 0]), devices(&["hd8"]));
}

#[test]
fn a_stepped_input_past_the_live_inputs_names_no_device() {
    assert!(stepped_input_devices(&devices(&["hd8"]), &[3]).is_empty());
}
