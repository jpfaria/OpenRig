//! #833 over the MCP surface: an agent driving OpenRig through MCP must hit
//! the same guard the GUI hits — the second chain on one physical input
//! (device + channel) fails to enable, with an explanatory error.
//!
//! The whole MCP path is exercised: tool name → args → `build_command` →
//! `CommandBridge` → the real `LocalDispatcher` on a frontend thread. Only the
//! HTTP transport is left out (it carries no OpenRig logic — `call_tool` turns
//! exactly this `Err` into `CallToolResult::error`).

use std::cell::RefCell;
use std::rc::Rc;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::mpsc;
use std::sync::Arc;
use std::time::Duration;

use adapter_mcp::tools::dispatch_tool;
use application::bridge;
use application::local_dispatcher::LocalDispatcher;
use domain::ids::{ChainId, DeviceId};
use domain::io_binding::{ChannelMode, IoBinding, IoEndpoint};
use infra_filesystem::FilesystemStorage;
use project::chain::Chain;
use project::project::Project;
use serde_json::json;

/// A binding exposing one mono input endpoint on `device` / `channel`.
fn input_binding(id: &str, device: &str, channel: usize) -> IoBinding {
    IoBinding {
        id: id.to_string(),
        name: id.to_string(),
        inputs: vec![IoEndpoint {
            name: format!("In {channel}"),
            device_id: DeviceId(device.to_string()),
            mode: ChannelMode::Mono,
            channels: vec![channel],
        }],
        outputs: vec![],
    }
}

fn chain_with_bindings(id: &str, enabled: bool, binding_ids: &[&str]) -> Chain {
    Chain {
        id: ChainId(id.to_string()),
        description: None,
        instrument: "electric_guitar".to_string(),
        enabled,
        volume: 100.0,
        io_binding_ids: binding_ids.iter().map(|s| s.to_string()).collect(),
        blocks: vec![],
        loopers: vec![],
        di_output: None,
    }
}

/// Outcome of one MCP tool call plus the resulting `enabled` flags, read on the
/// frontend thread that owns the (`!Send`) project.
struct McpRun {
    outcome: Result<String, String>,
    enabled: Vec<bool>,
}

/// Seed a config registry, run one MCP tool call against a live dispatcher.
fn run_tool_over_mcp(
    chains: Vec<Chain>,
    bindings: Vec<IoBinding>,
    tool: &str,
    args: serde_json::Value,
) -> McpRun {
    let tmp = tempfile::TempDir::new().expect("tempdir");
    let cfg_path = tmp.path().join("config.yaml");
    FilesystemStorage::update_app_config_at(&cfg_path, |config| {
        config.io_bindings = bindings;
    })
    .expect("seed config.yaml");

    let (bridge, drain) = bridge::channel();
    let stop = Arc::new(AtomicBool::new(false));
    let (tx, rx) = mpsc::channel();

    // The frontend thread owns the !Send dispatcher and pumps the bridge —
    // exactly what `adapter-gui` does while the MCP server runs beside it.
    let frontend = {
        let stop = Arc::clone(&stop);
        std::thread::spawn(move || {
            let project = Rc::new(RefCell::new(Project {
                name: None,
                device_settings: Vec::new(),
                chains,
                midi: None,
            }));
            let dispatcher = LocalDispatcher::new(Rc::clone(&project));
            dispatcher.attach_io_config_path(Some(cfg_path));
            while !stop.load(Ordering::Relaxed) {
                drain.drain(&dispatcher, 16);
                std::thread::sleep(Duration::from_millis(1));
            }
            drain.drain(&dispatcher, 16);
            let enabled = project.borrow().chains.iter().map(|c| c.enabled).collect();
            tx.send(enabled).expect("report enabled flags");
        })
    };

    let outcome = futures::executor::block_on(dispatch_tool(&bridge, tool, args))
        .map(|events| serde_json::to_string(&events).expect("events serialize"))
        .map_err(|e| e.to_string());

    stop.store(true, Ordering::Relaxed);
    let enabled = rx
        .recv_timeout(Duration::from_secs(5))
        .expect("frontend reply");
    frontend.join().expect("frontend thread");
    McpRun { outcome, enabled }
}

#[test]
fn mcp_toggle_chain_enabled_rejects_a_second_chain_on_one_physical_input() {
    let run = run_tool_over_mcp(
        vec![
            chain_with_bindings("chain_a", true, &["io_a"]),
            chain_with_bindings("chain_b", false, &["io_b"]),
        ],
        vec![
            input_binding("io_a", "scarlett", 0),
            input_binding("io_b", "scarlett", 0),
        ],
        "toggle_chain_enabled",
        json!({ "chain": "chain_b" }),
    );

    let error = run
        .outcome
        .expect_err("the MCP tool call must fail for the second chain on scarlett ch0");
    assert!(
        error.contains("scarlett") && error.contains("chain_a"),
        "the MCP error must name the contended input and its holder, got: {error}"
    );
    assert_eq!(
        run.enabled,
        vec![true, false],
        "chain_a stays enabled, chain_b must NOT have been enabled"
    );
}

#[test]
fn mcp_toggle_chain_enabled_still_enables_a_chain_on_a_free_input() {
    let run = run_tool_over_mcp(
        vec![
            chain_with_bindings("chain_a", true, &["io_a"]),
            chain_with_bindings("chain_b", false, &["io_b"]),
        ],
        vec![
            input_binding("io_a", "scarlett", 0),
            input_binding("io_b", "scarlett", 1),
        ],
        "toggle_chain_enabled",
        json!({ "chain": "chain_b" }),
    );

    assert!(
        run.outcome.is_ok(),
        "a free channel must still enable over MCP, got {:?}",
        run.outcome
    );
    assert_eq!(run.enabled, vec![true, true], "both chains must be enabled");
}
