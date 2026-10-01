//! Global mixer domain rules (issue #1007).

use crate::ids::DeviceId;
use crate::io_binding::{ChannelMode, IoBinding, IoEndpoint};
use crate::mixer_gain::{clamp_gain_db, strip_linear_gain, GAIN_DB_MAX, GAIN_DB_MIN};
use crate::mixer_strip::{MixerDirection, MixerStripId};
use crate::mixer_strips::strips_from_bindings;

const HD8: &str = "coreaudio:TUSBAudio:Fender:Quantum HD 8:1ed8:0210:0:QT9E25260495:11";

fn ep(name: &str, device: &str, mode: ChannelMode, channels: Vec<usize>) -> IoEndpoint {
    IoEndpoint {
        name: name.into(),
        device_id: DeviceId(device.into()),
        mode,
        channels,
    }
}

fn binding(id: &str, inputs: Vec<IoEndpoint>, outputs: Vec<IoEndpoint>) -> IoBinding {
    IoBinding {
        id: id.into(),
        name: id.into(),
        inputs,
        outputs,
    }
}

#[test]
fn strip_id_round_trips_through_its_wire_form() {
    let id = MixerStripId {
        direction: MixerDirection::Output,
        device_id: HD8.into(),
        channels: vec![24, 25],
    };
    let wire = id.to_wire();
    assert_eq!(wire, format!("out:24,25@{HD8}"));
    assert_eq!(MixerStripId::parse(&wire), Some(id));
}

#[test]
fn strip_id_parse_splits_at_the_first_at_sign() {
    let parsed = MixerStripId::parse("in:0@dev@with@ats").expect("valid id");
    assert_eq!(parsed.direction, MixerDirection::Input);
    assert_eq!(parsed.device_id, "dev@with@ats");
    assert_eq!(parsed.channels, vec![0]);
}

#[test]
fn strip_id_parse_rejects_garbage() {
    for bad in ["", "out", "out:@dev", "mid:0@dev", "out:a,b@dev", "out:0,1"] {
        assert_eq!(MixerStripId::parse(bad), None, "{bad:?} must not parse");
    }
}

#[test]
fn strip_id_without_a_device_does_not_parse() {
    // A strip addresses ONE device endpoint: no device, no strip.
    assert_eq!(MixerStripId::parse("in:0@"), None);
    assert_eq!(MixerStripId::parse("out:0,1@"), None);
}

#[test]
fn mono_input_with_several_channels_reaches_every_split_pipeline() {
    let id = MixerStripId {
        direction: MixerDirection::Input,
        device_id: "dev".into(),
        channels: vec![2, 3],
    };
    assert_eq!(
        id.runtime_channel_groups(ChannelMode::Mono),
        vec![vec![2, 3], vec![2], vec![3]]
    );
    assert_eq!(
        id.runtime_channel_groups(ChannelMode::Stereo),
        vec![vec![2, 3]]
    );
}

#[test]
fn output_strip_is_one_group_whatever_its_mode() {
    let id = MixerStripId {
        direction: MixerDirection::Output,
        device_id: "dev".into(),
        channels: vec![0, 1],
    };
    assert_eq!(
        id.runtime_channel_groups(ChannelMode::Mono),
        vec![vec![0, 1]]
    );
}

#[test]
fn the_same_physical_output_in_several_bindings_is_one_strip() {
    let main = || ep("MAIN (Out 1/2)", HD8, ChannelMode::Stereo, vec![0, 1]);
    let bindings = vec![
        binding(
            "guitarra-1",
            vec![ep("Guitar 1", HD8, ChannelMode::Mono, vec![0])],
            vec![main()],
        ),
        binding(
            "guitarra-2",
            vec![ep("Guitar 2", HD8, ChannelMode::Mono, vec![1])],
            vec![
                ep("Main again", HD8, ChannelMode::Stereo, vec![0, 1]),
                ep("FRFR", HD8, ChannelMode::Stereo, vec![24, 25]),
            ],
        ),
    ];
    let strips = strips_from_bindings(&bindings);
    let wires: Vec<String> = strips.iter().map(|s| s.id.to_wire()).collect();
    assert_eq!(
        wires,
        vec![
            format!("in:0@{HD8}"),
            format!("in:1@{HD8}"),
            format!("out:0,1@{HD8}"),
            format!("out:24,25@{HD8}"),
        ]
    );
    assert_eq!(strips[2].name, "MAIN (Out 1/2)");
    assert_eq!(strips[1].mode, ChannelMode::Mono);
}

#[test]
fn fader_range_is_clamped() {
    assert_eq!(clamp_gain_db(-200.0), GAIN_DB_MIN);
    assert_eq!(clamp_gain_db(40.0), GAIN_DB_MAX);
    assert_eq!(clamp_gain_db(-6.0), -6.0);
    assert_eq!(clamp_gain_db(f32::NAN), 0.0);
}

#[test]
fn unity_is_exactly_one_and_mute_is_exactly_zero() {
    assert_eq!(strip_linear_gain(0.0, false), 1.0);
    assert_eq!(strip_linear_gain(0.0, true), 0.0);
    assert_eq!(strip_linear_gain(6.0, true), 0.0);
    let minus_six = strip_linear_gain(-6.0, false);
    assert!((minus_six - 0.501_187).abs() < 1e-5, "{minus_six}");
    // The bottom of the fader is silence, not -60 dB of leakage.
    assert_eq!(strip_linear_gain(GAIN_DB_MIN, false), 0.0);
}
