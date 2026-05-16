//! Tests for `plan_ports` (issue #457 — LV2 plugins with output
//! control ports crashed on chain start because the port partitioner
//! discarded `ControlOut` ports, leaving them unconnected → SIGSEGV).

use super::*;
use plugin_loader::dispatch::Lv2Port;

fn port(index: usize, symbol: &str, role: Lv2PortRole) -> Lv2Port {
    Lv2Port {
        index,
        symbol: symbol.to_string(),
        role,
        default_value: None,
        minimum: None,
        maximum: None,
        name: None,
        is_toggle: false,
        is_integer: false,
        is_enumeration: false,
        scale_points: Vec::new(),
        range_steps: None,
    }
}

#[test]
fn control_out_port_is_routed_to_extra_out() {
    // The regression: a ControlOut port must NOT be silently dropped —
    // it has to be connected (via extra_out) or the plugin SIGSEGVs.
    let ports = vec![port(4, "attenuation", Lv2PortRole::ControlOut)];

    let plan = plan_ports(&ports, &ParameterSet::default());

    assert_eq!(
        plan.extra_out,
        vec![4],
        "ControlOut port must land in extra_out so it gets connected"
    );
}

#[test]
fn control_out_is_never_treated_as_input_control() {
    let ports = vec![port(4, "gr_meter", Lv2PortRole::ControlOut)];

    let plan = plan_ports(&ports, &ParameterSet::default());

    assert!(
        plan.control.is_empty(),
        "output control ports must not be fed as input control values"
    );
}

#[test]
fn tap_deesser_layout_connects_the_attenuation_meter() {
    // Exact shape of the reported plugin (TAP DeEsser, issue #457):
    // audio in=0, audio out=1, threshold/freq control in (2,3),
    // attenuation meter ControlOut=4.
    let ports = vec![
        port(0, "audio_in", Lv2PortRole::AudioIn),
        port(1, "audio_out", Lv2PortRole::AudioOut),
        port(2, "threshold", Lv2PortRole::ControlIn),
        port(3, "frequency", Lv2PortRole::ControlIn),
        port(4, "attenuation", Lv2PortRole::ControlOut),
    ];

    let plan = plan_ports(&ports, &ParameterSet::default());

    assert_eq!(plan.audio_in, vec![0]);
    assert_eq!(plan.audio_out, vec![1]);
    assert_eq!(
        plan.control.iter().map(|(i, _)| *i).collect::<Vec<_>>(),
        vec![2, 3]
    );
    assert_eq!(plan.extra_out, vec![4]);
}

#[test]
fn every_role_lands_in_exactly_one_bucket() {
    // Registry-style guard: a mixed port set must partition cleanly so
    // no future role change leaks a port into the wrong bucket.
    let ports = vec![
        port(0, "in_l", Lv2PortRole::AudioIn),
        port(1, "in_r", Lv2PortRole::AudioIn),
        port(2, "out_l", Lv2PortRole::AudioOut),
        port(3, "out_r", Lv2PortRole::AudioOut),
        port(4, "gain", Lv2PortRole::ControlIn),
        port(5, "midi_in", Lv2PortRole::AtomIn),
        port(6, "midi_out", Lv2PortRole::AtomOut),
        port(7, "latency", Lv2PortRole::ControlOut),
        port(8, "gr_meter", Lv2PortRole::ControlOut),
        port(9, "unknown", Lv2PortRole::Other),
    ];

    let plan = plan_ports(&ports, &ParameterSet::default());

    assert_eq!(plan.audio_in, vec![0, 1]);
    assert_eq!(plan.audio_out, vec![2, 3]);
    assert_eq!(
        plan.control.iter().map(|(i, _)| *i).collect::<Vec<_>>(),
        vec![4]
    );
    assert_eq!(plan.atom, vec![5, 6]);
    assert_eq!(plan.extra_out, vec![7, 8]);
}
