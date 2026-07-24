//! Issue #693 — reads must behave like an API: fully concurrent, never
//! queued behind the frontend thread.
//!
//! Today a transport query (`CommandBridge::query`) is only answered
//! when the GUI's 16 ms poll timer calls `serve_queries` on the
//! frontend thread — every MCP read waits for the screen, and a busy
//! screen (runtime build, modal, stall) starves all readers.
//!
//! Contract under test: after a command has gone through the
//! dispatcher, a `ProjectYaml` query resolves WITHOUT the frontend
//! ever ticking — served from the published state snapshot on the
//! caller's own thread, like a GET against a cache.

use std::cell::RefCell;
use std::rc::Rc;
use std::time::{Duration, Instant};

use application::bridge::{self, QueryKind};
use application::command::{Command, ProjectCommand};
use application::dispatcher::CommandDispatcher;
use application::local_dispatcher::LocalDispatcher;
use application::publishing_dispatcher::PublishingDispatcher;
use project::project::Project;

#[test]
fn issue_693_project_yaml_query_resolves_without_frontend_tick() {
    let (handle, _drain) = bridge::channel();

    // A live session that has already dispatched at least one command.
    let inner = LocalDispatcher::new(Rc::new(RefCell::new(Project {
        name: None,
        device_settings: Vec::new(),
        chains: Vec::new(),
        midi: None,
    })));
    let (sink, _events) = bridge::event_sink();
    let pd = PublishingDispatcher::new(inner, sink);
    pd.dispatch(Command::Project(ProjectCommand::UpdateProjectName {
        name: "issue 693 api reads".into(),
    }))
    .expect("dispatch UpdateProjectName");

    // The frontend NEVER drains queries here — no `serve_queries`, no
    // GUI tick. The read must still resolve.
    let mut rx = handle.query(QueryKind::ProjectYaml);
    let deadline = Instant::now() + Duration::from_millis(800);
    loop {
        match rx.try_recv() {
            Ok(Some(result)) => {
                let yaml = result.expect("ProjectYaml query must succeed");
                assert!(
                    yaml.contains("issue 693 api reads"),
                    "snapshot must reflect the dispatched state, got:\n{yaml}"
                );
                return;
            }
            Ok(None) => {
                assert!(
                    Instant::now() < deadline,
                    "read is waiting on the frontend tick: queries serialize \
                     behind the GUI thread instead of serving from the state \
                     snapshot (issue #693)"
                );
                std::thread::sleep(Duration::from_millis(10));
            }
            Err(_) => panic!("query channel canceled"),
        }
    }
}
