use anyhow::{anyhow, Result};
use pedal_domain::ids::TrackId;
use pedal_nam::processor::NamProcessor;
use pedal_setup::block::{AudioBlockKind, SelectBlock};
use pedal_setup::io::{Input, Output};
use pedal_setup::setup::Setup;
use pedal_setup::track::Track;
use std::collections::{HashMap, VecDeque};
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};
const DEBUG_MIN_PEAK_TO_LOG: f32 = 0.01;
const DEBUG_LOG_INTERVAL_MS: u64 = 300;
const DEFAULT_QUEUE_CAPACITY_FRAMES: usize = 48_000;
pub struct TrackRuntimeState {
    pub queue: VecDeque<Vec<f32>>,
    pub last_print: Instant,
    pub processors: Vec<RuntimeProcessor>,
}
pub enum RuntimeProcessor {
    Nam(NamProcessor),
}
pub struct RuntimeGraph {
    pub tracks: HashMap<TrackId, Arc<Mutex<TrackRuntimeState>>>,
}
pub fn build_runtime_graph(setup: &Setup) -> Result<RuntimeGraph> {
    let mut tracks = HashMap::new();
    for track in &setup.tracks {
        tracks.insert(
            track.id.clone(),
            Arc::new(Mutex::new(TrackRuntimeState {
                queue: VecDeque::with_capacity(DEFAULT_QUEUE_CAPACITY_FRAMES),
                last_print: Instant::now(),
                processors: build_runtime_processors(track)?,
            })),
        );
    }
    Ok(RuntimeGraph { tracks })
}
fn build_runtime_processors(track: &Track) -> Result<Vec<RuntimeProcessor>> {
    let mut processors = Vec::new();
    for block in &track.blocks {
        match &block.kind {
            AudioBlockKind::Nam(stage) => {
                println!("[track:{}] loading NAM '{}'", track.id.0, stage.model_path);
                if let Some(ir_path) = &stage.ir_path {
                    println!("[track:{}] IR configured but not yet applied: {}", track.id.0, ir_path);
                }
                processors.push(RuntimeProcessor::Nam(NamProcessor::new(&stage.model_path)?));
            }
            AudioBlockKind::Select(select) => {
                processors.push(RuntimeProcessor::Nam(load_selected_nam(track, select)?));
            }
            _ => {}
        }
    }
    Ok(processors)
}
fn load_selected_nam(track: &Track, select: &SelectBlock) -> Result<NamProcessor> {
    let selected = select
        .options
        .iter()
        .find(|option| option.id == select.selected_block_id)
        .ok_or_else(|| anyhow!("track '{}' select block references unknown option", track.id.0))?;
    match &selected.kind {
        AudioBlockKind::Nam(stage) => {
            println!("[track:{}] loading selected NAM '{}'", track.id.0, stage.model_path);
            if let Some(ir_path) = &stage.ir_path {
                println!("[track:{}] selected IR configured but not yet applied: {}", track.id.0, ir_path);
            }
            NamProcessor::new(&stage.model_path)
        }
        other => Err(anyhow!("track '{}' select block chose unsupported option: {:?}", track.id.0, other)),
    }
}
pub fn process_input_f32(
    track: &Track,
    input_cfg: &Input,
    runtime: &Arc<Mutex<TrackRuntimeState>>,
    data: &[f32],
    input_total_channels: usize,
) {
    let mut peak = 0.0f32;
    let mut locked = runtime.lock().expect("track runtime poisoned");
    for frame in data.chunks(input_total_channels) {
        let mut track_frame = Vec::with_capacity(input_cfg.channels.len());
        for &channel_index in &input_cfg.channels {
            let sample = frame.get(channel_index).copied().unwrap_or(0.0);
            let abs = sample.abs();
            if abs > peak {
                peak = abs;
            }
            track_frame.push(sample);
        }
        locked.queue.push_back(track_frame);
        if locked.queue.len() > DEFAULT_QUEUE_CAPACITY_FRAMES {
            locked.queue.pop_front();
        }
    }
    if peak >= DEBUG_MIN_PEAK_TO_LOG
        && locked.last_print.elapsed() >= Duration::from_millis(DEBUG_LOG_INTERVAL_MS)
    {
        println!(
            "[{}] audio detected | input_channels={:?} | peak={:.4} | queued={}",
            track.id.0,
            input_cfg.channels,
            peak,
            locked.queue.len()
        );
        locked.last_print = Instant::now();
    }
}
pub fn process_output_f32(
    track: &Track,
    output_cfg: &Output,
    runtime: &Arc<Mutex<TrackRuntimeState>>,
    out: &mut [f32],
    output_total_channels: usize,
) {
    let mut locked = runtime.lock().expect("track runtime poisoned");
    for frame in out.chunks_mut(output_total_channels) {
        frame.fill(0.0);
        let Some(input_samples) = locked.queue.pop_front() else {
            continue;
        };
        if input_samples.is_empty() {
            continue;
        }
        let mut sample = if input_samples.len() == 1 {
            input_samples[0]
        } else {
            input_samples.iter().copied().sum::<f32>() / input_samples.len() as f32
        };
        for processor in &mut locked.processors {
            match processor {
                RuntimeProcessor::Nam(processor) => {
                    sample = processor.process_sample(sample);
                }
            }
        }
        sample *= track.gain;
        for &channel_index in &output_cfg.channels {
            if let Some(dst) = frame.get_mut(channel_index) {
                *dst = sample;
            }
        }
    }
}
