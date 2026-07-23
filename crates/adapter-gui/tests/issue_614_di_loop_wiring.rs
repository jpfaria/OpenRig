//! Task 6 — RED-FIRST tests for adapter-gui DI loop wiring.
//!
//! ## Piece A — pure apply helper
//! `apply_di_loop_event(rt, arc_opt, enabled)` is a small pure function the
//! wiring closure will call after resolving the chain's `ChainRuntimeState`.
//! We verify it sets / clears `rt.has_di_loop()` correctly.
//!
//! ## Piece B — pure UI intent → command mapper
//! `di_loop_commands(chain, intent)` maps the four UI intents to `Vec<Command>`
//! without touching `AppWindow`.  The four intents mirror the task spec:
//!   - `PlayWithNewSource { source }` ⇒ `[SetChainDiLoopSource, SetChainDiLoopEnabled(true)]`
//!   - `Play`                          ⇒ `[SetChainDiLoopEnabled(true)]`
//!   - `Stop`                          ⇒ `[SetChainDiLoopEnabled(false)]`
//!   - `SelectSource { source }`       ⇒ `[SetChainDiLoopSource]`

use std::sync::Arc;

use application::command::Command;
use application::di_loader::DiLoopSource;
use domain::ids::ChainId;
use engine::di_loop::{DiLoop, DiPcm};
use engine::runtime::{build_chain_runtime_state, DEFAULT_ELASTIC_TARGET};
use project::block::AudioBlock;
use project::chain::Chain;

// ── helpers ────────────────────────────────────────────────────────────────

fn test_chain(id: &str) -> Chain {
    Chain {
        id: ChainId(id.into()),
        description: None,
        instrument: "electric_guitar".to_string(),
        enabled: true,
        volume: 100.0,
        io_binding_ids: vec![],
        blocks: Vec::<AudioBlock>::new(),
        di_output: None,
        loopers: vec![],
    }
}

fn build_rt(chain: &Chain) -> Arc<engine::runtime::ChainRuntimeState> {
    Arc::new(
        // #716: build_chain_runtime_state takes a trailing binding registry;
        // this chain is unbound (empty io_binding_ids), so pass an empty one.
        build_chain_runtime_state(chain, 48_000.0, &[DEFAULT_ELASTIC_TARGET], &[])
            .expect("build_chain_runtime_state"),
    )
}

fn dummy_di() -> Arc<DiLoop> {
    Arc::new(DiLoop::from_samples(&[0.0, 0.5, 1.0], 48_000, 1, 48_000, 0))
}

/// The un-resampled source `apply_di_loop_event` now receives (#749); it
/// resamples per runtime rate internally.
fn dummy_pcm() -> Arc<DiPcm> {
    Arc::new(DiPcm::new(vec![0.0, 0.5, 1.0], 48_000, 1))
}

fn dummy_source() -> DiLoopSource {
    DiLoopSource::File(std::path::PathBuf::from("/tmp/di.wav"))
}

// ── Piece A: apply_di_loop_event ────────────────────────────────────────────

/// `enabled = true` with a loaded arc must arm the runtime.
#[test]
fn apply_di_loop_event_enabled_true_with_arc_arms_runtime() {
    let chain = test_chain("chain_a");
    let rt = build_rt(&chain);
    let pcm = dummy_pcm();

    assert!(!rt.has_di_loop(), "precondition: no DI loop loaded");

    adapter_gui::di_loop_wiring::apply_di_loop_event(&rt, Some(pcm), true);

    assert!(
        rt.has_di_loop(),
        "has_di_loop must be true after enabled=true with arc"
    );
}

/// `enabled = true` with no arc (source not loaded yet) must leave the runtime
/// unchanged — the UI should never call enable before loading a source.
#[test]
fn apply_di_loop_event_enabled_true_without_arc_leaves_runtime_unchanged() {
    let chain = test_chain("chain_b");
    let rt = build_rt(&chain);

    adapter_gui::di_loop_wiring::apply_di_loop_event(&rt, None, true);

    assert!(!rt.has_di_loop(), "no arc → runtime must stay unchanged");
}

/// `enabled = false` must always clear the runtime, regardless of arc.
#[test]
fn apply_di_loop_event_enabled_false_clears_runtime() {
    let chain = test_chain("chain_c");
    let rt = build_rt(&chain);

    // Arm it first (directly with a built loop).
    rt.set_di_loop(Some(dummy_di()));
    assert!(rt.has_di_loop(), "precondition: DI loop armed");

    adapter_gui::di_loop_wiring::apply_di_loop_event(&rt, Some(dummy_pcm()), false);

    assert!(
        !rt.has_di_loop(),
        "has_di_loop must be false after enabled=false"
    );
}

// ── Piece B: di_loop_commands ────────────────────────────────────────────────

/// Play with a new source → [SetChainDiLoopSource, SetChainDiLoopEnabled(true)]
#[test]
fn di_loop_commands_play_with_new_source_returns_two_commands() {
    use adapter_gui::di_loop_wiring::{di_loop_commands, DiLoopIntent};

    let chain = ChainId("chain_x".into());
    let source = dummy_source();
    let cmds = di_loop_commands(
        chain.clone(),
        DiLoopIntent::PlayWithNewSource {
            source: source.clone(),
        },
    );

    assert_eq!(cmds.len(), 2, "expected 2 commands, got {cmds:?}");
    assert!(
        matches!(&cmds[0], Command::SetChainDiLoopSource { chain: c, source: s }
            if c.0 == "chain_x" && matches!(s, DiLoopSource::File(_))),
        "first command must be SetChainDiLoopSource, got {:?}",
        cmds[0]
    );
    assert!(
        matches!(&cmds[1], Command::SetChainDiLoopEnabled { chain: c, enabled: true }
            if c.0 == "chain_x"),
        "second command must be SetChainDiLoopEnabled(true), got {:?}",
        cmds[1]
    );
}

/// Play (source already loaded) → [SetChainDiLoopEnabled(true)]
#[test]
fn di_loop_commands_play_returns_enable_command() {
    use adapter_gui::di_loop_wiring::{di_loop_commands, DiLoopIntent};

    let chain = ChainId("chain_y".into());
    let cmds = di_loop_commands(chain.clone(), DiLoopIntent::Play);

    assert_eq!(cmds.len(), 1, "expected 1 command, got {cmds:?}");
    assert!(
        matches!(&cmds[0], Command::SetChainDiLoopEnabled { chain: c, enabled: true }
            if c.0 == "chain_y"),
        "command must be SetChainDiLoopEnabled(true), got {:?}",
        cmds[0]
    );
}

/// Stop → [SetChainDiLoopEnabled(false)]
#[test]
fn di_loop_commands_stop_returns_disable_command() {
    use adapter_gui::di_loop_wiring::{di_loop_commands, DiLoopIntent};

    let chain = ChainId("chain_z".into());
    let cmds = di_loop_commands(chain.clone(), DiLoopIntent::Stop);

    assert_eq!(cmds.len(), 1, "expected 1 command, got {cmds:?}");
    assert!(
        matches!(&cmds[0], Command::SetChainDiLoopEnabled { chain: c, enabled: false }
            if c.0 == "chain_z"),
        "command must be SetChainDiLoopEnabled(false), got {:?}",
        cmds[0]
    );
}

/// SelectSource → [SetChainDiLoopSource]
#[test]
fn di_loop_commands_select_source_returns_source_command() {
    use adapter_gui::di_loop_wiring::{di_loop_commands, DiLoopIntent};

    let chain = ChainId("chain_w".into());
    let source = dummy_source();
    let cmds = di_loop_commands(
        chain.clone(),
        DiLoopIntent::SelectSource {
            source: source.clone(),
        },
    );

    assert_eq!(cmds.len(), 1, "expected 1 command, got {cmds:?}");
    assert!(
        matches!(&cmds[0], Command::SetChainDiLoopSource { chain: c, source: s }
            if c.0 == "chain_w" && matches!(s, DiLoopSource::File(_))),
        "command must be SetChainDiLoopSource, got {:?}",
        cmds[0]
    );
}
