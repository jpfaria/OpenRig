//! The knob vocabulary and the output-endpoint resolution.
//!
//! #127: the tap-tempo history and the config→settings restore that used to be
//! tested here moved with the state itself, to
//! `application::metronome_state` — same assertions, one owner.

use super::*;

use engine::metronome_state::{MetronomeSettings, Subdivision, Timbre};

// ── knob index ↔ command key ─────────────────────────────────────────────

#[test]
fn the_time_signature_knob_walks_the_seven_supported_bars() {
    let beats: Vec<u32> = (0..7).map(time_signature_beats).collect();
    assert_eq!(beats, vec![2, 3, 4, 5, 6, 7, 12]);
}

#[test]
fn the_time_signature_knob_rests_on_four_four() {
    let beats = MetronomeSettings::default().beats_per_bar;
    assert_eq!(beats, 4);
    assert_eq!(time_signature_index(beats), 2);
    assert_eq!(time_signature_label(beats), "4/4");
}

#[test]
fn a_bar_length_the_knob_cannot_express_falls_back_to_four_four() {
    // An MCP client may ask for 9 beats; the knob has no position for it, so it
    // must not point at a random one.
    assert_eq!(time_signature_index(9), 2);
}

#[test]
fn the_subdivision_knob_speaks_the_commands_dispatcher_vocabulary() {
    let keys: Vec<&str> = (0..4).map(subdivision_key).collect();
    assert_eq!(keys, vec!["off", "eighths", "triplets", "sixteenths"]);
}

#[test]
fn the_timbre_knob_speaks_the_commands_dispatcher_vocabulary() {
    let keys: Vec<&str> = (0..3).map(timbre_key).collect();
    assert_eq!(keys, vec!["click", "wood", "beep"]);
}

#[test]
fn a_knob_index_past_the_last_position_stays_on_the_last_one() {
    // The knob cycles in Slint, but nothing may produce an out-of-range key.
    assert_eq!(subdivision_key(99), "sixteenths");
    assert_eq!(timbre_key(-1), "click");
    assert_eq!(time_signature_beats(99), 12);
}

#[test]
fn an_event_key_comes_back_as_the_knob_position_that_produced_it() {
    assert_eq!(subdivision_index("triplets"), 2);
    assert_eq!(subdivision_label("triplets"), "1/8T");
    assert_eq!(timbre_index("beep"), 2);
}

#[test]
fn a_snapshot_from_the_dispatcher_lands_on_every_knob() {
    // Whatever the dispatcher owns is what the window shows — including a
    // value only another transport could have set.
    let settings = MetronomeSettings {
        bpm: 96.0,
        beats_per_bar: 6,
        subdivision: Subdivision::Sixteenths,
        timbre: Timbre::Wood,
        volume: 0.4,
        count_in: true,
    };
    assert_eq!(time_signature_index(settings.beats_per_bar), 4);
    assert_eq!(time_signature_label(settings.beats_per_bar), "6/8");
    assert_eq!(subdivision_index(settings.subdivision.key()), 3);
    assert_eq!(timbre_index(settings.timbre.key()), 1);
}

#[test]
fn a_key_no_knob_position_carries_rests_on_the_first_one() {
    // The dispatcher rejects an unknown key on a command, but a hand-edited
    // `config.yaml` still has to render — on a real position, never at random.
    assert_eq!(subdivision_index("quintuplets"), 0);
    assert_eq!(subdivision_label("quintuplets"), "1/4");
    assert_eq!(timbre_index("gong"), 0);
}

// ── #14: output selection from the project's I/O bindings ─────────────────

use infra_filesystem::{ChannelMode, IoBinding, IoEndpoint};

fn binding(id: &str, name: &str, outputs: Vec<IoEndpoint>) -> IoBinding {
    IoBinding {
        id: id.into(),
        name: name.into(),
        inputs: vec![],
        outputs,
    }
}

fn endpoint(name: &str, device: &str, channels: Vec<usize>) -> IoEndpoint {
    IoEndpoint {
        name: name.into(),
        device_id: domain::ids::DeviceId(device.into()),
        mode: ChannelMode::Stereo,
        channels,
    }
}

#[test]
fn output_endpoints_flattens_every_bindings_outputs() {
    let bindings = vec![
        binding(
            "main",
            "Scarlett 2i2",
            vec![endpoint("Main Out 1-2", "dev:scarlett", vec![0, 1])],
        ),
        binding(
            "monitor",
            "Headphones",
            vec![
                endpoint("Phones L", "dev:hp", vec![0]),
                endpoint("Phones R", "dev:hp", vec![1]),
            ],
        ),
    ];
    let outs = output_endpoints(&bindings);
    assert_eq!(
        outs.len(),
        3,
        "one entry per output endpoint across bindings"
    );
    assert_eq!(outs[0].label, "Scarlett 2i2 · Main Out 1-2");
    assert_eq!(outs[0].device_id, "dev:scarlett");
    assert_eq!(outs[0].channels, vec![0, 1]);
    assert_eq!(outs[2].label, "Headphones · Phones R");
    assert_eq!(outs[2].channels, vec![1]);
    // Keys are unique so the select can round-trip a pick.
    assert_ne!(outs[1].key, outs[2].key);
}

#[test]
fn resolve_output_endpoint_prefers_the_saved_one() {
    let bindings = vec![binding(
        "main",
        "Scarlett",
        vec![
            endpoint("Out A", "dev:x", vec![0, 1]),
            endpoint("Out B", "dev:x", vec![2, 3]),
        ],
    )];
    let outs = output_endpoints(&bindings);
    let saved = outs[1].key.clone();
    let picked = resolve_output_endpoint(Some(&saved), &outs).expect("saved endpoint resolves");
    assert_eq!(picked.channels, vec![2, 3], "the saved endpoint is chosen");
}

#[test]
fn resolve_output_endpoint_falls_back_to_the_first() {
    let bindings = vec![binding(
        "main",
        "Scarlett",
        vec![endpoint("Out A", "dev:x", vec![0, 1])],
    )];
    let outs = output_endpoints(&bindings);
    // A saved key from another machine / a renamed binding no longer resolves.
    let picked = resolve_output_endpoint(Some("gone::whatever"), &outs)
        .expect("falls back rather than going silent");
    assert_eq!(picked.channels, vec![0, 1]);
    // And with nothing saved.
    assert!(resolve_output_endpoint(None, &outs).is_some());
}

#[test]
fn resolve_output_endpoint_is_none_without_any_output() {
    assert!(resolve_output_endpoint(Some("main::x"), &[]).is_none());
    assert!(resolve_output_endpoint(None, &[]).is_none());
}
