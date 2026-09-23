//! Tests for the insert topology rule (#967).

use super::{insert_cuts_chain, insert_owns_streams};
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
fn a_bound_insert_owns_its_streams_whether_or_not_it_is_enabled() {
    assert!(insert_owns_streams(&insert("fx", true), &registry()));
    assert!(
        insert_owns_streams(&insert("fx", false), &registry()),
        "switching the loop off must not close its send and return"
    );
}

#[test]
fn only_an_enabled_bound_insert_cuts_the_chain() {
    assert!(insert_cuts_chain(&insert("fx", true), &registry()));
    assert!(
        !insert_cuts_chain(&insert("fx", false), &registry()),
        "a disabled insert is a pass-through, as it always was"
    );
}

#[test]
fn an_unbound_insert_owns_nothing_and_cuts_nothing() {
    for enabled in [true, false] {
        assert!(!insert_owns_streams(&insert("gone", enabled), &registry()));
        assert!(!insert_cuts_chain(&insert("gone", enabled), &registry()));
    }
}

#[test]
fn an_effect_is_not_an_insert() {
    let effect = AudioBlock {
        id: BlockId("gain".into()),
        enabled: true,
        kind: AudioBlockKind::Core(CoreBlock {
            effect_type: "gain".into(),
            model: "volume".into(),
            params: ParameterSet::default(),
        }),
    };
    assert!(!insert_owns_streams(&effect, &registry()));
    assert!(!insert_cuts_chain(&effect, &registry()));
}
