//! Responsibility: serves the MCP bridge from the Slint event loop.
//!
//! A complementary network server on the live instance: an agent drives the
//! same `ProjectSession` the user has open. The server runs on its own thread
//! (tokio); commands and queries cross the `!Send` boundary through the
//! bridge and are serviced here on the Slint event-loop thread — the same
//! place GUI callbacks dispatch — so GUI and MCP share one project with no
//! lock. The returned timer must live for the whole `window.run()`.

use anyhow::Result;
use slint::{ComponentHandle, Timer};
use std::cell::RefCell;
use std::net::SocketAddr;
use std::rc::Rc;

use crate::spectrum_session::SpectrumSession;
use crate::state::ProjectSession;
use crate::tuner_session::TunerSession;
use crate::AppWindow;

pub(crate) struct McpDeps {
    pub project_runtime: Rc<RefCell<Option<infra_cpal::ProjectRuntimeController>>>,
    pub tuner_session: Rc<RefCell<Option<TunerSession>>>,
    pub spectrum_session: Rc<RefCell<Option<SpectrumSession>>>,
}

pub(crate) fn start(
    addr: SocketAddr,
    window: &AppWindow,
    project_session: &Rc<RefCell<Option<ProjectSession>>>,
    nav_ctx: crate::chain_rig_nav_wiring::ChainRigNavCtx,
    deps: McpDeps,
) -> Result<Timer> {
    let (bridge, drain) = application::bridge::channel();
    std::thread::Builder::new()
        .name("openrig-mcp".into())
        .spawn(move || {
            if let Err(e) = adapter_mcp::run_blocking(bridge, addr) {
                log::error!("MCP server stopped: {e}");
            }
        })?;
    log::info!("MCP server listening on http://{addr}");
    let session_for_mcp = project_session.clone();
    let mcp_ctx = nav_ctx;
    let mcp_window = window.as_weak();
    // Cloned for the meter resolver closure (the moves above
    // consumed `project_chains` for the rig-nav ctx).
    let chains_for_meters = mcp_ctx.project_chains.clone();
    // #829: analyzer readings are served from the same live sessions the
    // Tuner / Spectrum windows render, so every transport reads the very
    // numbers on screen instead of a parallel derivation.
    let tuner_for_queries = deps.tuner_session;
    let spectrum_for_queries = deps.spectrum_session;
    let runtime_for_queries = deps.project_runtime;
    let timer = Timer::default();
    timer.start(
        slint::TimerMode::Repeated,
        std::time::Duration::from_millis(16),
        move || {
            // Drain + serve queries under the session borrow, then drop
            // it before refreshing (apply_events_to_ui re-borrows it).
            let events = {
                let session_borrow = session_for_mcp.borrow();
                let Some(session) = session_borrow.as_ref() else {
                    return;
                };
                let mut events = drain.drain(session.dispatcher.as_ref(), 32);
                // #693: completions of off-thread command work (DI
                // decode, ...) ride the same event path as a dispatch.
                {
                    events.extend(session.dispatcher.poll_async_results());
                }
                drain.serve_queries(
                    |kind| {
                        crate::mcp_query_resolver::QueryResolver {
                            session,
                            chain_rows: &chains_for_meters,
                            tuner: &tuner_for_queries,
                            spectrum: &spectrum_for_queries,
                            runtime: &runtime_for_queries,
                        }
                        .resolve(kind)
                    },
                    32,
                );
                events
            };
            if events.is_empty() {
                return;
            }
            if let Some(window) = mcp_window.upgrade() {
                crate::chain_rig_nav_wiring::apply_events_to_ui(&window, &mcp_ctx, &events);
            }
        },
    );
    Ok(timer)
}
