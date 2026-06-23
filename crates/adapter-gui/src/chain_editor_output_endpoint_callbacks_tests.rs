use domain::ids::ChainId;

use application::command::Command;

use super::build_save_output_endpoints_cmd;

// ── select_io_endpoint_on_output_dispatches_save_output_endpoints (#716) ──────

#[test]
fn select_io_endpoint_on_output_dispatches_save_output_endpoints() {
    let chain = ChainId("chain:2".into());
    let cmd = build_save_output_endpoints_cmd(chain.clone(), 1, "main", "Out1");
    match cmd {
        Command::SaveChainOutputEndpoints {
            chain: c,
            block_index,
            io,
            endpoint,
        } => {
            assert_eq!(c, chain);
            assert_eq!(block_index, 1);
            assert_eq!(io, "main");
            assert_eq!(endpoint, "Out1");
        }
        other => panic!("expected SaveChainOutputEndpoints, got {other:?}"),
    }
}

#[test]
fn select_io_endpoint_on_output_preserves_block_index() {
    let chain = ChainId("chain:y".into());
    let cmd = build_save_output_endpoints_cmd(chain.clone(), 5, "fx", "Send");
    match cmd {
        Command::SaveChainOutputEndpoints {
            chain: c,
            block_index,
            io,
            endpoint,
        } => {
            assert_eq!(c, chain);
            assert_eq!(block_index, 5, "block_index must be forwarded as-is");
            assert_eq!(io, "fx");
            assert_eq!(endpoint, "Send");
        }
        other => panic!("expected SaveChainOutputEndpoints, got {other:?}"),
    }
}
