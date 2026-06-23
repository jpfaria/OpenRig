use domain::ids::ChainId;

use application::command::Command;

use super::build_save_input_endpoints_cmd;

// ── select_io_endpoint_on_input_dispatches_save_input_endpoints (#716) ────────

#[test]
fn select_io_endpoint_on_input_dispatches_save_input_endpoints() {
    let chain = ChainId("chain:1".into());
    let cmd = build_save_input_endpoints_cmd(chain.clone(), 0, "main", "In1");
    match cmd {
        Command::SaveChainInputEndpoints {
            chain: c,
            block_index,
            io,
            endpoint,
        } => {
            assert_eq!(c, chain);
            assert_eq!(block_index, 0);
            assert_eq!(io, "main");
            assert_eq!(endpoint, "In1");
        }
        other => panic!("expected SaveChainInputEndpoints, got {other:?}"),
    }
}

#[test]
fn select_io_endpoint_on_input_preserves_block_index() {
    let chain = ChainId("chain:x".into());
    let cmd = build_save_input_endpoints_cmd(chain.clone(), 3, "loop", "Guitar");
    match cmd {
        Command::SaveChainInputEndpoints {
            chain: c,
            block_index,
            io,
            endpoint,
        } => {
            assert_eq!(c, chain);
            assert_eq!(block_index, 3, "block_index must be forwarded as-is");
            assert_eq!(io, "loop");
            assert_eq!(endpoint, "Guitar");
        }
        other => panic!("expected SaveChainInputEndpoints, got {other:?}"),
    }
}
