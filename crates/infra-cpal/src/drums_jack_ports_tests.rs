use super::{drums_jack_ports, jack_server_for_device};
use crate::jack_route_ports::RoutePort;

fn port(name: &str, playback: &str) -> RoutePort {
    RoutePort {
        name: name.into(),
        playback: playback.into(),
    }
}

#[test]
fn the_drums_own_one_port_per_target_channel() {
    assert_eq!(
        drums_jack_ports(&[2, 3]),
        vec![
            port("out_1", "system:playback_3"),
            port("out_2", "system:playback_4"),
        ]
    );
}

#[test]
fn a_mono_endpoint_gets_one_port() {
    assert_eq!(
        drums_jack_ports(&[5]),
        vec![port("out_1", "system:playback_6")]
    );
}

#[test]
fn only_the_stereo_pair_gets_ports() {
    assert_eq!(drums_jack_ports(&[0, 1, 2]).len(), 2);
}

#[test]
fn no_target_plays_on_the_first_two_channels() {
    assert_eq!(
        drums_jack_ports(&[]),
        vec![
            port("out_1", "system:playback_1"),
            port("out_2", "system:playback_2"),
        ]
    );
}

#[test]
fn the_server_comes_from_the_endpoint_device() {
    let none = |_: &str| None;
    assert_eq!(
        jack_server_for_device("jack:gen", none).as_deref(),
        Some("gen")
    );
    let card = |num: &str| (num == "1").then(|| "card1".to_string());
    assert_eq!(
        jack_server_for_device("hw:1", card).as_deref(),
        Some("card1")
    );
    assert_eq!(jack_server_for_device("hw:7", card), None);
    assert_eq!(jack_server_for_device("coreaudio:x", none), None);
}
