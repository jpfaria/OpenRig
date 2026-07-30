//! Issue #127: MCP/MIDI and the GUI must reach the audio runtime by the SAME
//! road — the `Command` bus.
//!
//! `chain_rig_nav_wiring::apply_events_to_ui` is the drain every external
//! transport lands in (the MCP poll timer in `desktop_app.rs`, the MIDI
//! footswitch timer in `midi_adapter_wiring.rs`). It used to call
//! `sync_live_chain_runtime` directly while the GUI's own callbacks dispatched a
//! `Command`, so the two frontends' requests travelled different code paths that
//! only happened to end in the same function — the split that lets one road
//! silently gain (or lose) a step the other does not have. Every UI callback
//! dispatches; so must the drain.

use std::path::PathBuf;

fn drain_source() -> String {
    let path = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("src/chain_rig_nav_wiring.rs");
    std::fs::read_to_string(&path).unwrap_or_else(|e| panic!("read {}: {e}", path.display()))
}

/// The chain-sync loop inside `apply_events_to_ui`, bounded from the comment
/// that opens it to the DI-loop application that follows — so the assertions
/// cannot pass by matching something elsewhere in the module.
fn chain_sync_loop(src: &str) -> String {
    let start = src
        .find("let mut synced: Vec<ChainId>")
        .expect("chain_rig_nav_wiring.rs has no per-chain sync loop");
    let rest = &src[start..];
    let end = rest
        .find("// Apply DI-loop events")
        .expect("expected the DI-loop application to follow the sync loop");
    rest[..end].to_string()
}

#[test]
fn the_external_event_drain_asks_for_the_chain_sync_on_the_bus() {
    let src = drain_source();
    let body = chain_sync_loop(&src);
    assert!(
        body.contains("ChainCommand::SyncChainRuntime"),
        "the MCP/MIDI drain must dispatch `ChainCommand::SyncChainRuntime` like \
         every UI callback does, so both roads to the audio runtime are the \
         same one. Loop read:\n{body}"
    );
    assert!(
        !body.contains("sync_live_chain_runtime("),
        "the drain still reaches `sync_live_chain_runtime` off the bus — that is \
         the GUI/MCP split #127 exists to close. Loop read:\n{body}"
    );
}

/// The drain is the first place an external transport can touch this frontend,
/// possibly before any chain sync has attached the runtime control. Dispatching
/// into a dispatcher that cannot reach the audio would make the request a
/// silent no-op, so the drain guarantees the attach itself.
#[test]
fn the_drain_hands_the_dispatcher_the_runtime_before_dispatching_on_it() {
    let src = drain_source();
    let attach = src
        .find("attach_runtime_control")
        .expect("the drain must attach this frontend's runtime control");
    let sync = src
        .find("ChainCommand::SyncChainRuntime")
        .expect("the drain must dispatch the sync on the bus");
    assert!(
        attach < sync,
        "the runtime control must be attached BEFORE the drain dispatches a \
         runtime-control command, or the very first external command of a \
         freshly opened project is swallowed"
    );
}
