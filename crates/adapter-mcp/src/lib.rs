//! Frontend-agnostic MCP server. Owns no state; drives the live OpenRig
//! instance through `application::bridge::CommandBridge`. Streamable HTTP
//! transport — connects to the already-running frontend (GUI or console),
//! coexisting with it (both share one `ProjectSession`).

mod prompts;
pub mod render_tool;
pub mod resources;
mod server;
mod tools;

use std::net::SocketAddr;

use anyhow::Result;
use application::bridge::CommandBridge;

pub use server::OpenRigMcp;

/// Run the MCP server (Streamable HTTP) until the process exits. Call from a
/// dedicated thread; this builds its own tokio runtime.
pub fn run_blocking(bridge: CommandBridge, addr: SocketAddr) -> Result<()> {
    let rt = tokio::runtime::Builder::new_multi_thread()
        .enable_all()
        .build()?;
    rt.block_on(server::serve(bridge, addr))
}
