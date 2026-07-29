//! Red-first (#576) integration tests for `ChainCommand::RenderChain`.
//!
//! Render is exposed through the Command bus so MCP/gRPC/any future
//! transport adapter inherits the tool automatically via the
//! schema-derived catalog in `application::command_schema` — the same
//! parity contract as every other Command. The handler lives in
//! `application::render` (file mode only; live capture stays in the
//! `openrig-render` binary so `application` does not pick up a `cpal`
//! dep).

use std::cell::RefCell;
use std::path::{Path, PathBuf};
use std::rc::Rc;

use application::command::{ChainCommand, Command};
use application::dispatcher::CommandDispatcher;
use application::event::Event;
use application::local_dispatcher::LocalDispatcher;
use project::project::Project;

fn workdir(test: &str) -> PathBuf {
    let mut p = std::env::temp_dir();
    p.push(format!(
        "openrig-render-chain-cmd-{}-{test}",
        std::process::id()
    ));
    let _ = std::fs::remove_dir_all(&p);
    std::fs::create_dir_all(&p).unwrap();
    p
}

fn write_passthrough_chain(dir: &Path) -> PathBuf {
    let yaml = r#"id: vol-100
name: passthrough
blocks:
- type: gain
  model: volume
  enabled: true
  params:
    volume: 100.0
    mute: false
"#;
    let path = dir.join("chain.yaml");
    std::fs::write(&path, yaml).unwrap();
    path
}

fn write_dc_wav_48k(path: &Path, frames: usize, value: f32) {
    use std::io::Write;
    // Minimal WAV writer (32-bit float, stereo, 48 kHz) — keeps this
    // test file free of an `adapter-render` dev-dep cycle.
    let sr: u32 = 48_000;
    let channels: u16 = 2;
    let bits: u16 = 32;
    let byte_rate = sr * (channels as u32) * (bits as u32) / 8;
    let block_align = channels * bits / 8;
    let data_bytes = (frames as u32) * (block_align as u32);
    let mut f = std::fs::File::create(path).unwrap();
    f.write_all(b"RIFF").unwrap();
    f.write_all(&(36 + data_bytes).to_le_bytes()).unwrap();
    f.write_all(b"WAVE").unwrap();
    f.write_all(b"fmt ").unwrap();
    f.write_all(&16u32.to_le_bytes()).unwrap();
    f.write_all(&3u16.to_le_bytes()).unwrap(); // IEEE float
    f.write_all(&channels.to_le_bytes()).unwrap();
    f.write_all(&sr.to_le_bytes()).unwrap();
    f.write_all(&byte_rate.to_le_bytes()).unwrap();
    f.write_all(&block_align.to_le_bytes()).unwrap();
    f.write_all(&bits.to_le_bytes()).unwrap();
    f.write_all(b"data").unwrap();
    f.write_all(&data_bytes.to_le_bytes()).unwrap();
    for _ in 0..frames {
        for _ in 0..channels {
            f.write_all(&value.to_le_bytes()).unwrap();
        }
    }
}

fn empty_project() -> Rc<RefCell<Project>> {
    Rc::new(RefCell::new(Project {
        name: Some("issue-576".into()),
        device_settings: Vec::new(),
        chains: Vec::new(),
        midi: None,
    }))
}

#[test]
fn render_chain_command_writes_output_wav_and_emits_completed_event() {
    let dir = workdir("happy_path");
    let chain = write_passthrough_chain(&dir);
    let input = dir.join("in.wav");
    let output = dir.join("out.wav");
    write_dc_wav_48k(&input, 4_800, 0.5);

    let project = empty_project();
    let dispatcher = LocalDispatcher::new(Rc::clone(&project));

    let events = dispatcher
        .dispatch(Command::Chain(ChainCommand::RenderChain {
            chain_path: chain.to_string_lossy().into_owned(),
            input_path: input.to_string_lossy().into_owned(),
            output_path: output.to_string_lossy().into_owned(),
            start_s: None,
            end_s: None,
            sample_rate_hz: None,
            block_size: None,
            bit_depth: None,
            tail_ms: Some(0),
        }))
        .expect("RenderChain dispatch succeeds");

    // #693: the render runs on its own task — the completion event
    // arrives via poll_async_results (the frontend tick's job).
    let mut events = events;
    let deadline = std::time::Instant::now() + std::time::Duration::from_secs(10);
    while !events
        .iter()
        .any(|e| matches!(e, Event::RenderCompleted { .. } | Event::Error { .. }))
        && std::time::Instant::now() < deadline
    {
        std::thread::sleep(std::time::Duration::from_millis(20));
        events.extend(dispatcher.poll_async_results());
    }

    assert!(
        output.exists(),
        "ChainCommand::RenderChain must write the output WAV"
    );
    let render_event = events.iter().find_map(|e| match e {
        Event::RenderCompleted {
            output_path,
            duration_seconds,
            sample_rate,
            bit_depth,
        } => Some((
            output_path.clone(),
            *duration_seconds,
            *sample_rate,
            *bit_depth,
        )),
        _ => None,
    });
    let (out_path, duration_s, sr, bits) =
        render_event.expect("dispatcher must emit Event::RenderCompleted");
    assert_eq!(PathBuf::from(&out_path), output);
    assert!(duration_s > 0.0);
    assert_eq!(sr, 48_000);
    assert_eq!(bits, 24);
}

#[test]
fn render_chain_command_with_invalid_bit_depth_errors_with_no_partial_output() {
    let dir = workdir("bad_bit_depth");
    let chain = write_passthrough_chain(&dir);
    let input = dir.join("in.wav");
    let output = dir.join("out.wav");
    write_dc_wav_48k(&input, 1_000, 0.1);

    let project = empty_project();
    let dispatcher = LocalDispatcher::new(Rc::clone(&project));

    let result = dispatcher.dispatch(Command::Chain(ChainCommand::RenderChain {
        chain_path: chain.to_string_lossy().into_owned(),
        input_path: input.to_string_lossy().into_owned(),
        output_path: output.to_string_lossy().into_owned(),
        start_s: None,
        end_s: None,
        sample_rate_hz: None,
        block_size: None,
        bit_depth: Some(19),
        tail_ms: Some(0),
    }));
    assert!(
        result.is_err(),
        "bit_depth=19 must be rejected (valid set is 16|24|32)"
    );
    assert!(
        !output.exists(),
        "invalid args must not leave a partial output WAV behind"
    );
}

#[test]
fn render_chain_command_with_missing_input_wav_errors_with_no_partial_output() {
    let dir = workdir("missing_input");
    let chain = write_passthrough_chain(&dir);
    let input = dir.join("does-not-exist.wav");
    let output = dir.join("out.wav");

    let project = empty_project();
    let dispatcher = LocalDispatcher::new(Rc::clone(&project));

    let result = dispatcher.dispatch(Command::Chain(ChainCommand::RenderChain {
        chain_path: chain.to_string_lossy().into_owned(),
        input_path: input.to_string_lossy().into_owned(),
        output_path: output.to_string_lossy().into_owned(),
        start_s: None,
        end_s: None,
        sample_rate_hz: None,
        block_size: None,
        bit_depth: None,
        tail_ms: Some(0),
    }));
    assert!(
        result.is_err(),
        "missing input WAV must fail (no live capture in the Command path)"
    );
    assert!(
        !output.exists(),
        "render failure must not leave a partial output WAV behind"
    );
}
