//! #328 — Linux/JACK-direct: every output route of a chain's runtime owns its
//! own JACK output ports, each connected to the playback port of its channel.
//! Two routes on one channel are summed by JACK, never by our code.

use super::{route_ports, RoutePorts};

fn names(group: &RoutePorts) -> Vec<&str> {
    group.ports.iter().map(|p| p.name.as_str()).collect()
}

#[test]
fn every_route_gets_its_own_ports() {
    let groups = route_ports(2, 4);
    assert_eq!(
        groups.iter().map(|g| g.route).collect::<Vec<_>>(),
        vec![0, 1],
        "#328: route 1 (a Y chain's path-B output) needs its own ports"
    );
    for group in &groups {
        assert_eq!(group.ports.len(), 4);
    }
    let mut all: Vec<&str> = groups.iter().flat_map(names).collect();
    let before = all.len();
    all.sort_unstable();
    all.dedup();
    assert_eq!(all.len(), before, "#328: no port is shared by two routes");
}

#[test]
fn each_route_port_connects_to_the_playback_port_of_its_channel() {
    for group in route_ports(2, 4) {
        let playbacks: Vec<&str> = group.ports.iter().map(|p| p.playback.as_str()).collect();
        assert_eq!(
            playbacks,
            vec![
                "system:playback_1",
                "system:playback_2",
                "system:playback_3",
                "system:playback_4"
            ],
            "#328: route {} must reach the device's playback ports, where JACK sums",
            group.route
        );
    }
}

/// Every JACK chain before #328 registered `out_1..out_N`; route 0 keeps
/// exactly those names so a single-output chain is unchanged.
#[test]
fn route_0_keeps_the_legacy_port_names() {
    let groups = route_ports(1, 2);
    assert_eq!(groups.len(), 1);
    assert_eq!(names(&groups[0]), vec!["out_1", "out_2"]);
}

#[test]
fn a_runtime_without_routes_still_gets_the_legacy_ports() {
    let groups = route_ports(0, 2);
    assert_eq!(groups.len(), 1);
    assert_eq!(groups[0].route, 0);
    assert_eq!(names(&groups[0]), vec!["out_1", "out_2"]);
}
