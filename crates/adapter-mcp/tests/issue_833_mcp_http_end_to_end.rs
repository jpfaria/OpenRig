//! #833 validated over the REAL MCP wire: a raw JSON-RPC client speaking
//! Streamable HTTP to `adapter_mcp::run_blocking`, against a live
//! `LocalDispatcher` on a frontend thread — the exact topology the GUI runs
//! (`--mcp`), minus the GUI.
//!
//! The sibling test (`issue_833_channel_conflict_over_mcp.rs`) stops at
//! `dispatch_tool`. This one adds the transport: HTTP request → session →
//! `tools/call` → `CallToolResult`, and reads the resulting state back through
//! `resources/read openrig://ids`. Writes AND reads travel over MCP, so nothing
//! here can pass by looking at in-process memory the wire never saw.

use std::cell::RefCell;
use std::io::{Read, Write};
use std::net::{SocketAddr, TcpListener, TcpStream};
use std::rc::Rc;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Arc;
use std::thread::JoinHandle;
use std::time::{Duration, Instant};

use application::bridge::{self, QueryKind};
use application::local_dispatcher::LocalDispatcher;
use domain::ids::{ChainId, DeviceId};
use domain::io_binding::{ChannelMode, IoBinding, IoEndpoint};
use infra_filesystem::FilesystemStorage;
use project::chain::Chain;
use project::project::Project;
use serde_json::{json, Value};

// ---------------------------------------------------------------- fixtures

fn input_binding(id: &str, device: &str, channels: Vec<usize>) -> IoBinding {
    IoBinding {
        id: id.to_string(),
        name: id.to_string(),
        inputs: vec![IoEndpoint {
            name: format!("In {id}"),
            device_id: DeviceId(device.to_string()),
            mode: if channels.len() > 1 {
                ChannelMode::Stereo
            } else {
                ChannelMode::Mono
            },
            channels,
        }],
        outputs: vec![],
    }
}

fn chain(id: &str, enabled: bool, binding_ids: &[&str]) -> Chain {
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

fn project_with(chains: Vec<Chain>) -> Project {
    Project {
        name: None,
        device_settings: Vec::new(),
        chains,
        midi: None,
    }
}

// ------------------------------------------------------------ HTTP client

/// One HTTP exchange: connect, write the request, read to EOF. `Connection:
/// close` keeps framing trivial — whatever hyper picks (chunked or SSE), the
/// payload is the first JSON-RPC object in the body.
fn post(addr: SocketAddr, session: Option<&str>, body: &str) -> (Vec<(String, String)>, String) {
    let mut stream = TcpStream::connect(addr).expect("connect to the MCP server");
    stream
        .set_read_timeout(Some(Duration::from_secs(10)))
        .expect("read timeout");
    let session_header = session
        .map(|s| format!("Mcp-Session-Id: {s}\r\n"))
        .unwrap_or_default();
    let request = format!(
        "POST / HTTP/1.1\r\nHost: {addr}\r\nContent-Type: application/json\r\n\
         Accept: application/json, text/event-stream\r\n{session_header}\
         Content-Length: {len}\r\nConnection: close\r\n\r\n{body}",
        len = body.len()
    );
    stream
        .write_all(request.as_bytes())
        .expect("write the request");
    let mut raw = Vec::new();
    stream.read_to_end(&mut raw).expect("read the response");
    let raw = String::from_utf8_lossy(&raw).into_owned();
    let (head, body) = raw
        .split_once("\r\n\r\n")
        .expect("HTTP response with a header block");
    let headers = head
        .lines()
        .skip(1)
        .filter_map(|line| line.split_once(':'))
        .map(|(k, v)| (k.trim().to_ascii_lowercase(), v.trim().to_string()))
        .collect();
    (headers, body.to_string())
}

/// The first JSON-RPC object in a response body, whether it arrived as plain
/// JSON, as `data:` SSE frames, or wrapped in HTTP chunk markers.
fn json_rpc(body: &str) -> Value {
    let start = body
        .find("{\"jsonrpc\"")
        .unwrap_or_else(|| panic!("no JSON-RPC payload in: {body}"));
    serde_json::Deserializer::from_str(&body[start..])
        .into_iter::<Value>()
        .next()
        .expect("a JSON value")
        .expect("well-formed JSON-RPC")
}

// ---------------------------------------------------------------- harness

/// A live OpenRig MCP endpoint: frontend thread (owning the `!Send`
/// dispatcher) + HTTP server + an initialized MCP session.
struct McpEndpoint {
    addr: SocketAddr,
    session: String,
    stop: Arc<AtomicBool>,
    frontend: Option<JoinHandle<()>>,
}

impl McpEndpoint {
    fn start(chains: Vec<Chain>, bindings: Vec<IoBinding>) -> Self {
        let tmp = tempfile::TempDir::new().expect("tempdir");
        let cfg_path = tmp.path().join("config.yaml");
        FilesystemStorage::update_app_config_at(&cfg_path, |config| {
            config.io_bindings = bindings;
        })
        .expect("seed config.yaml");

        // Reserve a free port, then hand it to the server.
        let addr = TcpListener::bind("127.0.0.1:0")
            .expect("reserve a port")
            .local_addr()
            .expect("local addr");

        let (bridge, drain) = bridge::channel();
        let stop = Arc::new(AtomicBool::new(false));

        let frontend = {
            let stop = Arc::clone(&stop);
            std::thread::spawn(move || {
                let project = Rc::new(RefCell::new(project_with(chains)));
                let dispatcher = LocalDispatcher::new(Rc::clone(&project));
                dispatcher.attach_io_config_path(Some(cfg_path));
                // Hold the tempdir for the frontend's lifetime.
                let _tmp = tmp;
                while !stop.load(Ordering::Relaxed) {
                    drain.drain(&dispatcher, 16);
                    drain.serve_queries(
                        |kind| match kind {
                            QueryKind::Ids => Ok(application::query::list_ids(&project.borrow())),
                            other => Err(format!("unsupported in this test: {other:?}")),
                        },
                        16,
                    );
                    std::thread::sleep(Duration::from_millis(1));
                }
            })
        };

        std::thread::spawn(move || {
            let _ = adapter_mcp::run_blocking(bridge, addr);
        });

        let deadline = Instant::now() + Duration::from_secs(10);
        while TcpStream::connect(addr).is_err() {
            assert!(Instant::now() < deadline, "MCP server never bound {addr}");
            std::thread::sleep(Duration::from_millis(20));
        }

        let (headers, body) = post(
            addr,
            None,
            &json!({
                "jsonrpc": "2.0", "id": 1, "method": "initialize",
                "params": {
                    "protocolVersion": "2025-06-18",
                    "capabilities": {},
                    "clientInfo": { "name": "issue-833-http-test", "version": "0" }
                }
            })
            .to_string(),
        );
        assert!(
            json_rpc(&body).get("result").is_some(),
            "initialize failed: {body}"
        );
        let session = headers
            .iter()
            .find(|(k, _)| k == "mcp-session-id")
            .map(|(_, v)| v.clone())
            .expect("the server must issue a session id");
        post(
            addr,
            Some(&session),
            r#"{"jsonrpc":"2.0","method":"notifications/initialized"}"#,
        );

        Self {
            addr,
            session,
            stop,
            frontend: Some(frontend),
        }
    }

    /// `tools/call` → `(is_error, text payload)`.
    fn call_tool(&self, name: &str, arguments: Value) -> (bool, String) {
        let (_, body) = post(
            self.addr,
            Some(&self.session),
            &json!({
                "jsonrpc": "2.0", "id": 2, "method": "tools/call",
                "params": { "name": name, "arguments": arguments }
            })
            .to_string(),
        );
        let response = json_rpc(&body);
        let result = response
            .get("result")
            .unwrap_or_else(|| panic!("tools/call {name} returned no result: {response}"));
        let text = result["content"][0]["text"]
            .as_str()
            .unwrap_or_default()
            .to_string();
        (result["isError"].as_bool().unwrap_or(false), text)
    }

    /// `resources/read openrig://ids` → `(chain id, enabled)` in project
    /// order. The state check travels over MCP too — no peeking at the
    /// in-process project the wire never saw.
    fn enabled_flags(&self) -> Vec<(String, bool)> {
        let (_, body) = post(
            self.addr,
            Some(&self.session),
            &json!({
                "jsonrpc": "2.0", "id": 3, "method": "resources/read",
                "params": { "uri": "openrig://ids" }
            })
            .to_string(),
        );
        let response = json_rpc(&body);
        let listing = response["result"]["contents"][0]["text"]
            .as_str()
            .unwrap_or_else(|| panic!("no ids payload: {response}"));
        // Lines look like: `chain <id>  instrument=<x>  enabled|disabled`
        listing
            .lines()
            .filter_map(|line| line.strip_prefix("chain "))
            .map(|line| {
                let mut parts = line.split_whitespace();
                let id = parts.next().expect("chain id").to_string();
                let state = parts.last().expect("chain state");
                (id, state == "enabled")
            })
            .collect()
    }
}

impl Drop for McpEndpoint {
    fn drop(&mut self) {
        self.stop.store(true, Ordering::Relaxed);
        if let Some(frontend) = self.frontend.take() {
            let _ = frontend.join();
        }
    }
}

// ------------------------------------------------------------------ tests

/// Checklist item 1: two chains on one physical capture point — the second
/// enable is refused, with an error naming the device and the holder.
#[test]
fn mcp_http_refuses_the_second_chain_on_one_device_channel() {
    let mcp = McpEndpoint::start(
        vec![
            chain("chain_a", true, &["io_a"]),
            chain("chain_b", false, &["io_b"]),
        ],
        vec![
            input_binding("io_a", "scarlett", vec![0]),
            input_binding("io_b", "scarlett", vec![0]),
        ],
    );

    let (is_error, text) = mcp.call_tool("toggle_chain_enabled", json!({ "chain": "chain_b" }));

    assert!(is_error, "the MCP call must report an error, got: {text}");
    assert!(
        text.contains("scarlett") && text.contains("chain_a"),
        "the error must name the contended input and its holder, got: {text}"
    );
    assert_eq!(
        mcp.enabled_flags(),
        vec![
            ("chain_a".to_string(), true),
            ("chain_b".to_string(), false)
        ],
        "chain_b must not have been enabled"
    );
}

/// Checklist items 3 and 4: the same interface on another channel, and
/// another interface on the same channel index, are independent taps.
#[test]
fn mcp_http_allows_distinct_channels_and_distinct_devices() {
    let mcp = McpEndpoint::start(
        vec![
            chain("chain_a", true, &["io_a"]),
            chain("other_channel", false, &["io_same_device_ch1"]),
            chain("other_device", false, &["io_other_device_ch0"]),
        ],
        vec![
            input_binding("io_a", "scarlett", vec![0]),
            input_binding("io_same_device_ch1", "scarlett", vec![1]),
            input_binding("io_other_device_ch0", "teyun", vec![0]),
        ],
    );

    for chain_id in ["other_channel", "other_device"] {
        let (is_error, text) = mcp.call_tool("toggle_chain_enabled", json!({ "chain": chain_id }));
        assert!(!is_error, "{chain_id} must enable, got: {text}");
    }
    assert_eq!(
        mcp.enabled_flags(),
        vec![
            ("chain_a".to_string(), true),
            ("other_channel".to_string(), true),
            ("other_device".to_string(), true),
        ]
    );
}

/// Checklist item 5: freeing the capture point releases it — disabling the
/// holder lets the contender in.
#[test]
fn mcp_http_lets_the_contender_in_once_the_holder_is_disabled() {
    let mcp = McpEndpoint::start(
        vec![
            chain("chain_a", true, &["io_a"]),
            chain("chain_b", false, &["io_b"]),
        ],
        vec![
            input_binding("io_a", "scarlett", vec![0]),
            input_binding("io_b", "scarlett", vec![0]),
        ],
    );

    let (holder_off, text) = mcp.call_tool("toggle_chain_enabled", json!({ "chain": "chain_a" }));
    assert!(!holder_off, "disabling is never refused, got: {text}");
    let (is_error, text) = mcp.call_tool("toggle_chain_enabled", json!({ "chain": "chain_b" }));
    assert!(!is_error, "the freed channel must accept chain_b: {text}");
    assert_eq!(
        mcp.enabled_flags(),
        vec![
            ("chain_a".to_string(), false),
            ("chain_b".to_string(), true)
        ]
    );
}

/// Checklist item 6: re-binding an ENABLED chain onto a taken tap is refused
/// and leaves the binding untouched.
#[test]
fn mcp_http_refuses_rebinding_an_enabled_chain_onto_a_taken_tap() {
    let mcp = McpEndpoint::start(
        vec![
            chain("chain_a", true, &["io_a"]),
            chain("chain_b", true, &["io_free"]),
        ],
        vec![
            input_binding("io_a", "scarlett", vec![0]),
            input_binding("io_free", "scarlett", vec![1]),
        ],
    );

    let (is_error, text) = mcp.call_tool(
        "set_chain_io_bindings",
        json!({ "chain": "chain_b", "binding_ids": ["io_a"] }),
    );

    assert!(is_error, "re-binding onto scarlett ch0 must fail: {text}");
    assert!(
        text.contains("scarlett") && text.contains("chain_a"),
        "the error must name the contended input and its holder, got: {text}"
    );
    assert_eq!(
        mcp.enabled_flags(),
        vec![("chain_a".to_string(), true), ("chain_b".to_string(), true)],
        "both chains stay enabled on their own taps"
    );
}

/// Checklist item 7: a project that already carries two enabled chains on one
/// input (saved before the guard, or hand-edited) loads with the later one off.
#[test]
fn mcp_http_load_project_disables_the_conflicting_chain() {
    let mcp = McpEndpoint::start(
        vec![],
        vec![
            input_binding("io_a", "scarlett", vec![0]),
            input_binding("io_b", "scarlett", vec![0]),
            input_binding("io_free", "scarlett", vec![1]),
        ],
    );

    let incoming = project_with(vec![
        chain("first", true, &["io_a"]),
        chain("second", true, &["io_b"]),
        chain("free", true, &["io_free"]),
    ]);
    let (is_error, text) = mcp.call_tool(
        "load_project",
        json!({
            "project": serde_json::to_value(&incoming).expect("project serializes"),
            "path": "/tmp/issue-833-load.openrig",
        }),
    );

    assert!(!is_error, "loading must succeed, got: {text}");
    // Chain ids are `#[serde(skip)]`, so a loaded project carries fresh ones —
    // assert the order-preserving enabled pattern instead.
    assert_eq!(
        mcp.enabled_flags()
            .iter()
            .map(|(_, enabled)| *enabled)
            .collect::<Vec<_>>(),
        vec![true, false, true],
        "the second chain on scarlett ch0 must come up disabled"
    );
}
