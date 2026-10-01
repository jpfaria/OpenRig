use super::*;

#[test]
fn chain_accessor_returns_the_affected_chain() {
    // The MIDI/MCP refresh needs to know which chain each event
    // touched so it can re-sync that chain's live runtime.
    let c = ChainId("rig:guitar".into());
    assert_eq!(Event::ChainReloaded { chain: c.clone() }.chain(), Some(&c));
    assert_eq!(
        Event::ChainVolumeChanged {
            chain: c.clone(),
            value: 80.0
        }
        .chain(),
        Some(&c)
    );
    // Project-wide events carry no chain.
    assert_eq!(Event::ProjectSaved.chain(), None);
    assert_eq!(Event::ProjectMutated.chain(), None);
}

#[test]
fn a_saved_looper_take_belongs_to_its_chain() {
    // #827: saving a take re-syncs only the chain whose looper it came from.
    let c = ChainId("rig:guitar".into());
    assert_eq!(
        Event::ChainLooperTakeSaved {
            chain: c.clone(),
            looper: 1,
            path: std::path::PathBuf::from("verse.wav"),
        }
        .chain(),
        Some(&c)
    );
}

#[test]
fn chain_mixer_events_are_scoped_to_their_chain() {
    let c = ChainId("rig:guitar".into());
    assert_eq!(
        Event::ChainMixerStripChanged {
            chain: c.clone(),
            strip: "out:0,1@hd8".into(),
            gain_db: -6.0,
            muted: false,
        }
        .chain(),
        Some(&c)
    );
    assert_eq!(
        Event::ChainDiFaderChanged {
            chain: c.clone(),
            gain_db: -3.0,
        }
        .chain(),
        Some(&c)
    );
}

#[test]
fn a_deleted_looper_take_belongs_to_no_chain() {
    // #1021: the take library is app-wide; every chain's DI picker lists it.
    assert_eq!(
        Event::LooperTakeDeleted {
            path: std::path::PathBuf::from("verse.wav"),
        }
        .chain(),
        None
    );
}
