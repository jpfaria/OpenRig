//! Responsibility: runs bridge commands that need no project state on their own task.
//!
//! The frontend dispatch of `RenderChain` spawns the render and returns
//! `Ok([])` so the GUI tick never parks (#693) — which left a transport
//! (MCP/gRPC) with an empty reply and no way to learn the outcome (#938).
//! A command that touches no project state does not need the frontend at
//! all: the bridge runs it here and replies with the finished outcome.

use futures::channel::oneshot;

use crate::bridge::DispatchOutcome;
use crate::command::{ChainCommand, Command};

type Job = Box<dyn FnOnce() -> DispatchOutcome + Send>;

/// The work `cmd` stands for when it can run without the frontend.
pub(crate) fn off_frontend_job(cmd: &Command) -> Option<Job> {
    match cmd {
        Command::Chain(ChainCommand::RenderChain {
            chain_path,
            input_path,
            output_path,
            start_s,
            end_s,
            sample_rate_hz,
            block_size,
            bit_depth,
            tail_ms,
        }) => {
            let (chain_path, input_path, output_path) =
                (chain_path.clone(), input_path.clone(), output_path.clone());
            let (start_s, end_s, sample_rate_hz, block_size, bit_depth, tail_ms) =
                (*start_s, *end_s, *sample_rate_hz, *block_size, *bit_depth, *tail_ms);
            Some(Box::new(move || {
                crate::render_handler::precheck(bit_depth, &input_path)
                    .map_err(|e| e.to_string())?;
                crate::render_handler::run(
                    chain_path,
                    input_path,
                    output_path,
                    start_s,
                    end_s,
                    sample_rate_hz,
                    block_size,
                    bit_depth,
                    tail_ms,
                )
                .map(|event| vec![event])
                .map_err(|e| format!("RenderChain failed: {e}"))
            }))
        }
        _ => None,
    }
}

/// Run `job` on its own thread and reply with its outcome. If the thread
/// cannot be spawned the reply is dropped and the transport sees a
/// cancelled request instead of hanging.
pub(crate) fn spawn_with_reply(job: Job, reply: oneshot::Sender<DispatchOutcome>) {
    let _ = std::thread::Builder::new()
        .name("bridge-off-frontend".into())
        .spawn(move || {
            let _ = reply.send(job());
        });
}
