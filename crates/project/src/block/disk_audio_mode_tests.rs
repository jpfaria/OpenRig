use super::*;
use block_core::ModelAudioMode;
use plugin_loader::dispatch::{Lv2Port, Lv2PortRole};

fn port(index: usize, role: Lv2PortRole) -> Lv2Port {
    Lv2Port {
        index,
        symbol: format!("p{index}"),
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

fn audio_ports(inputs: usize, outputs: usize) -> Vec<Lv2Port> {
    let mut ports = vec![port(0, Lv2PortRole::ControlIn)];
    ports.extend((0..inputs).map(|i| port(1 + i, Lv2PortRole::AudioIn)));
    ports.extend((0..outputs).map(|i| port(1 + inputs + i, Lv2PortRole::AudioOut)));
    ports
}

#[test]
fn lv2_one_in_one_out_runs_dual_mono() {
    assert_eq!(lv2_audio_mode(&audio_ports(1, 1)), ModelAudioMode::DualMono);
}

#[test]
fn lv2_sidechain_two_in_one_out_runs_dual_mono() {
    assert_eq!(lv2_audio_mode(&audio_ports(2, 1)), ModelAudioMode::DualMono);
}

#[test]
fn lv2_one_in_two_out_runs_mono_to_stereo() {
    assert_eq!(
        lv2_audio_mode(&audio_ports(1, 2)),
        ModelAudioMode::MonoToStereo
    );
}

#[test]
fn lv2_two_in_two_out_runs_true_stereo() {
    assert_eq!(
        lv2_audio_mode(&audio_ports(2, 2)),
        ModelAudioMode::TrueStereo
    );
}
