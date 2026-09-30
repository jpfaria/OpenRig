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
