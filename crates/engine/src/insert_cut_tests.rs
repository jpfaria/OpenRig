//! Tests for the insert-cut rule (#967).

use super::insert_cuts_chain;
use domain::ids::{BlockId, DeviceId};
use domain::io_binding::{ChannelMode, IoBinding, IoEndpoint};
use project::block::{AudioBlock, AudioBlockKind, CoreBlock, InsertBlock};
use project::param::ParameterSet;

fn registry() -> Vec<IoBinding> {
    let ep = |ch| IoEndpoint {
        name: "e".into(),
        device_id: DeviceId("dev".into()),
        mode: ChannelMode::Mono,
        channels: vec![ch],
    };
    vec![IoBinding {
        id: "fx".into(),
        name: "FX".into(),
        inputs: vec![ep(3)],
        outputs: vec![ep(4)],
    }]
}

fn insert(io: &str, enabled: bool) -> AudioBlock {
    AudioBlock {
        id: BlockId("insert".into()),
        enabled,
        kind: AudioBlockKind::Insert(InsertBlock {
            model: "standard".into(),
            io: io.into(),
        }),
    }
}

#[test]
fn an_enabled_bound_insert_cuts_the_chain() {
    assert!(insert_cuts_chain(&insert("fx", true), &registry()));
}

#[cfg(not(all(target_os = "linux", feature = "jack")))]
#[test]
fn a_disabled_bound_insert_still_cuts_the_chain_on_cpal() {
    assert!(
        insert_cuts_chain(&insert("fx", false), &registry()),
        "switching the loop off bypasses it in the DSP; its streams stay"
    );
}

#[cfg(all(target_os = "linux", feature = "jack"))]
#[test]
fn a_disabled_bound_insert_does_not_cut_the_chain_on_jack() {
    assert!(
        !insert_cuts_chain(&insert("fx", false), &registry()),
        "JACK runs one input and one route per chain — it cannot bypass a cut"
    );
}

#[test]
fn an_unbound_insert_never_cuts_the_chain() {
    assert!(!insert_cuts_chain(&insert("gone", true), &registry()));
    assert!(!insert_cuts_chain(&insert("", false), &registry()));
}

#[test]
fn an_effect_never_cuts_the_chain() {
    let effect = AudioBlock {
        id: BlockId("gain".into()),
        enabled: true,
        kind: AudioBlockKind::Core(CoreBlock {
            effect_type: "gain".into(),
            model: "volume".into(),
            params: ParameterSet::default(),
        }),
    };
    assert!(!insert_cuts_chain(&effect, &registry()));
}
