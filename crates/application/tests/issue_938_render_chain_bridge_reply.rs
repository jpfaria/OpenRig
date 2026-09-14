//! Issue #938 — a transport (MCP/gRPC) that submits `RenderChain` through the
//! command bridge must get the render's OUTCOME back: `RenderCompleted` once
//! the WAV exists, or the render error. Since #693 the frontend dispatch
//! spawns the render and replies `Ok([])` at once, so the MCP tool printed
//! `[]`, never wrote a file the caller could wait for, and swallowed every
//! render failure.

use std::cell::RefCell;
use std::path::{Path, PathBuf};
use std::rc::Rc;
use std::time::{Duration, Instant};

use application::bridge::{channel, DispatchOutcome};
use application::command::{ChainCommand, Command};
use application::event::Event;
use application::local_dispatcher::LocalDispatcher;
use project::project::Project;

fn workdir(test: &str) -> PathBuf {
    let mut p = std::env::temp_dir();
    p.push(format!("openrig-938-bridge-{}-{test}", std::process::id()));
    let _ = std::fs::remove_dir_all(&p);
    std::fs::create_dir_all(&p).unwrap();
    p
}

fn write_chain(dir: &Path, yaml: &str) -> PathBuf {
    let path = dir.join("chain.yaml");
    std::fs::write(&path, yaml).unwrap();
    path
}

const PASSTHROUGH_CHAIN: &str = r#"id: vol-100
name: passthrough
blocks:
- type: gain
  model: volume
  enabled: true
  params:
    volume: 100.0
    mute: false
"#;

fn write_silent_wav_48k(path: &Path, frames: usize) {
    let spec = hound::WavSpec {
        channels: 2,
        sample_rate: 48_000,
        bits_per_sample: 32,
        sample_format: hound::SampleFormat::Float,
    };
    let mut writer = hound::WavWriter::create(path, spec).unwrap();
    for _ in 0..frames * 2 {
        writer.write_sample(0.0f32).unwrap();
    }
    writer.finalize().unwrap();
}

fn render_cmd(chain: &Path, input: &Path, output: &Path) -> Command {
    Command::Chain(ChainCommand::RenderChain {
        chain_path: chain.to_string_lossy().into_owned(),
        input_path: input.to_string_lossy().into_owned(),
        output_path: output.to_string_lossy().into_owned(),
        start_s: None,
        end_s: None,
        sample_rate_hz: None,
        block_size: None,
        bit_depth: None,
        tail_ms: Some(0),
    })
}

/// Submit through the bridge and keep the frontend ticking (drain +
/// async poll, exactly what the GUI timer does) until the reply resolves.
fn submit_and_wait(cmd: Command) -> DispatchOutcome {
    let project = Rc::new(RefCell::new(Project {
        name: Some("issue-938".into()),
        device_settings: Vec::new(),
        chains: Vec::new(),
        midi: None,
    }));
    let dispatcher = LocalDispatcher::new(project);
    let (bridge, drain) = channel();
    let mut reply = bridge.submit(cmd);
    let deadline = Instant::now() + Duration::from_secs(20);
    loop {
        drain.drain(&dispatcher, 8);
        let _ = application::dispatcher::CommandDispatcher::poll_async_results(&dispatcher);
        if let Ok(Some(outcome)) = reply.try_recv() {
            return outcome;
        }
        assert!(Instant::now() < deadline, "bridge reply never resolved");
        std::thread::sleep(Duration::from_millis(10));
    }
}

#[test]
fn bridge_render_reply_carries_completion_after_the_wav_exists() {
    let dir = workdir("ok");
    let chain = write_chain(&dir, PASSTHROUGH_CHAIN);
    let input = dir.join("in.wav");
    let output = dir.join("out.wav");
    write_silent_wav_48k(&input, 4_800);

    let events = submit_and_wait(render_cmd(&chain, &input, &output))
        .expect("a valid render must reply Ok");

    assert!(
        events
            .iter()
            .any(|e| matches!(e, Event::RenderCompleted { .. })),
        "the transport must receive RenderCompleted, got {events:?}"
    );
    assert!(
        output.exists(),
        "the WAV must exist when the reply arrives"
    );
}

#[test]
fn bridge_render_reply_carries_the_render_error() {
    let dir = workdir("err");
    let chain = write_chain(&dir, "this: is: not: a chain\n");
    let input = dir.join("in.wav");
    let output = dir.join("out.wav");
    write_silent_wav_48k(&input, 4_800);

    let outcome = submit_and_wait(render_cmd(&chain, &input, &output));

    let err = outcome.expect_err("a render that fails must reply Err, not Ok([])");
    assert!(
        err.contains("RenderChain failed"),
        "the reply must name the render failure, got: {err}"
    );
}
