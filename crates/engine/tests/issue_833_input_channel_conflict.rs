//! #833: naming the contended capture point, and normalizing a project that
//! already carries two enabled chains on one physical input.
//!
//! #716 gave the activation path `input_conflicting_chains` — it tells the
//! runtime which chains to SKIP, silently. That left the project able to hold
//! two "enabled" chains on one guitar: the runtime dropped the second, the user
//! saw both lit. These two functions close that gap on the same resolution:
//! `conflicting_input_channel` names the holder so a command can be refused,
//! `disable_conflicting_chains` makes a loaded project tell the truth.

use domain::ids::{ChainId, DeviceId};
use domain::io_binding::{ChannelMode, IoBinding, IoEndpoint};
use engine::runtime_endpoints::{
    conflicting_input_channel, disable_conflicting_chains, InputChannelConflict,
};
use project::chain::Chain;
use project::project::Project;

fn binding(id: &str, device: &str, channels: Vec<usize>) -> IoBinding {
    IoBinding {
        id: id.to_string(),
        name: id.to_string(),
        inputs: vec![IoEndpoint {
            name: "In".to_string(),
            device_id: DeviceId(device.to_string()),
            mode: if channels.len() > 1 {
                ChannelMode::Stereo
            } else {
                ChannelMode::Mono
            },
            channels,
        }],
        outputs: vec![],
    }
}

fn chain(id: &str, enabled: bool, binding_ids: &[&str]) -> Chain {
    Chain {
        id: ChainId(id.to_string()),
        description: None,
        instrument: "electric_guitar".to_string(),
        enabled,
        volume: 100.0,
        io_binding_ids: binding_ids.iter().map(|s| s.to_string()).collect(),
        blocks: vec![],
        di_output: None,
    }
}

fn project_with(chains: Vec<Chain>) -> Project {
    Project {
        name: None,
        device_settings: Vec::new(),
        chains,
        midi: None,
    }
}

#[test]
fn conflict_names_the_enabled_holder_and_the_contended_tap() {
    // Two DIFFERENT bindings on one physical capture point: the conflict is on
    // the resolved (device, channel), never on the binding id.
    let registry = vec![
        binding("io_a", "scarlett", vec![0]),
        binding("io_b", "scarlett", vec![0]),
    ];
    let holder = chain("holder", true, &["io_a"]);
    let candidate = chain("candidate", false, &["io_b"]);
    let chains = vec![holder, candidate.clone()];

    assert_eq!(
        conflicting_input_channel(&candidate, chains.iter(), &registry),
        Some(InputChannelConflict {
            chain: ChainId("holder".to_string()),
            device: "scarlett".to_string(),
            channel: 0,
        })
    );
}

#[test]
fn conflict_covers_any_shared_channel_of_a_multi_channel_endpoint() {
    let registry = vec![
        binding("io_stereo", "scarlett", vec![0, 1]),
        binding("io_mono", "scarlett", vec![1]),
    ];
    let holder = chain("holder", true, &["io_stereo"]);
    let candidate = chain("candidate", false, &["io_mono"]);
    let chains = vec![holder, candidate.clone()];

    let conflict = conflicting_input_channel(&candidate, chains.iter(), &registry)
        .expect("ch1 sits inside the holder's stereo pair");
    assert_eq!(conflict.channel, 1);
}

#[test]
fn no_conflict_for_the_candidate_itself_disabled_holders_or_distinct_taps() {
    let registry = vec![
        binding("io_a", "scarlett", vec![0]),
        binding("io_other_channel", "scarlett", vec![1]),
        binding("io_other_device", "teyun", vec![0]),
    ];
    let enabled_holder = chain("holder", true, &["io_a"]);
    let disabled_holder = chain("disabled", false, &["io_a"]);
    let same_tap = chain("same_tap", false, &["io_a"]);
    let other_channel = chain("other_channel", false, &["io_other_channel"]);
    let other_device = chain("other_device", false, &["io_other_device"]);

    // Only itself in the list → nothing to contend with.
    assert_eq!(
        conflicting_input_channel(&enabled_holder, [&enabled_holder], &registry),
        None
    );
    // A disabled chain holds nothing.
    assert_eq!(
        conflicting_input_channel(&same_tap, [&disabled_holder, &same_tap], &registry),
        None
    );
    // Distinct channel / distinct device are independent capture points.
    assert_eq!(
        conflicting_input_channel(&other_channel, [&enabled_holder, &other_channel], &registry),
        None
    );
    assert_eq!(
        conflicting_input_channel(&other_device, [&enabled_holder, &other_device], &registry),
        None
    );
}

#[test]
fn unresolvable_bindings_contend_with_nothing() {
    let candidate = chain("candidate", false, &["ghost"]);
    let holder = chain("holder", true, &["ghost"]);
    assert_eq!(
        conflicting_input_channel(&candidate, [&holder, &candidate], &[]),
        None
    );
}

#[test]
fn disable_conflicting_chains_keeps_the_first_and_flips_off_the_rest() {
    let registry = vec![
        binding("io_a", "scarlett", vec![0]),
        binding("io_b", "scarlett", vec![0]),
        binding("io_free", "scarlett", vec![1]),
    ];
    let mut project = project_with(vec![
        chain("first", true, &["io_a"]),
        chain("second", true, &["io_b"]),
        chain("free", true, &["io_free"]),
    ]);

    let disabled = disable_conflicting_chains(&mut project, &registry);

    assert_eq!(disabled, vec![ChainId("second".to_string())]);
    assert!(
        project.chains[0].enabled,
        "the first chain keeps the channel"
    );
    assert!(
        !project.chains[1].enabled,
        "the contender comes up disabled"
    );
    assert!(project.chains[2].enabled, "a free channel is untouched");
}

#[test]
fn disable_conflicting_chains_leaves_a_clean_project_alone() {
    let registry = vec![
        binding("io_a", "scarlett", vec![0]),
        binding("io_b", "scarlett", vec![1]),
    ];
    let mut project = project_with(vec![
        chain("a", true, &["io_a"]),
        chain("b", true, &["io_b"]),
        chain("off", false, &["io_a"]),
    ]);

    assert!(disable_conflicting_chains(&mut project, &registry).is_empty());
    assert_eq!(
        project.chains.iter().map(|c| c.enabled).collect::<Vec<_>>(),
        vec![true, true, false]
    );
}
