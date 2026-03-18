use anyhow::{anyhow, bail, Result};
use domain::ids::TrackId;
use setup::block::{AudioBlockKind, CoreBlockKind, SelectBlock};
use setup::io::{Input, Output};
use setup::setup::Setup;
use setup::track::Track;
use stage_core::MonoProcessor;
use stage_delay::digital::DigitalDelay;
use stage_delay::DelayParams;
use stage_dynamics::compressor::Compressor;
use stage_dynamics::gate::NoiseGate;
use stage_eq::ThreeBandEq;
use stage_modulation::tremolo::Tremolo;
use stage_nam::processor::NamProcessor;
use stage_reverb::plate::PlateReverb;
use stage_reverb::ReverbParams;
use stage_utility::tuner::ChromaticTuner;
use std::collections::{HashMap, VecDeque};
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};
const DEBUG_MIN_PEAK_TO_LOG: f32 = 0.01;
const DEBUG_LOG_INTERVAL_MS: u64 = 300;
const DEFAULT_QUEUE_CAPACITY_FRAMES: usize = 48_000;
const DEFAULT_SAMPLE_RATE: f32 = 48_000.0;
pub struct TrackRuntimeState {
    pub queue: VecDeque<Vec<f32>>,
    pub last_print: Instant,
    pub processors: Vec<RuntimeProcessor>,
}
pub enum RuntimeProcessor {
    Nam(NamProcessor),
    Delay(DigitalDelay),
    Reverb(PlateReverb),
    Tuner(ChromaticTuner),
    Compressor(Compressor),
    Gate(NoiseGate),
    Eq(ThreeBandEq),
    Tremolo(Tremolo),
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
            AudioBlockKind::Core(core) => match &core.kind {
                CoreBlockKind::Delay(stage) => {
                    let model = stage.model.as_str();
                    match model {
                        "native_digital" | "rust_style_digital" | "digital" => {
                            println!("[track:{}] loading delay model={} time_ms={} feedback={} mix={}", track.id.0, model, stage.time_ms, stage.feedback, stage.mix);
                            processors.push(RuntimeProcessor::Delay(DigitalDelay::new(
                                DelayParams {
                                    time_ms: stage.time_ms,
                                    feedback: stage.feedback,
                                    mix: stage.mix,
                                },
                                DEFAULT_SAMPLE_RATE,
                            )));
                        }
                        other => bail!("track '{}' uses unsupported delay model '{}'", track.id.0, other),
                    }
                }
                CoreBlockKind::Reverb(stage) => {
                    let model = stage.model.as_str();
                    match model {
                        "plate" | "spring" | "hall" | "room" => {
                            println!("[track:{}] loading reverb model={} room_size={} damping={} mix={}", track.id.0, model, stage.room_size, stage.damping, stage.mix);
                            processors.push(RuntimeProcessor::Reverb(PlateReverb::new(
                                ReverbParams {
                                    room_size: stage.room_size,
                                    damping: stage.damping,
                                    mix: stage.mix,
                                },
                                DEFAULT_SAMPLE_RATE,
                            )));
                        }
                        other => bail!("track '{}' uses unsupported reverb model '{}'", track.id.0, other),
                    }
                }
                CoreBlockKind::Tuner(stage) => {
                    let model = stage.model.as_str();
                    match model {
                        "chromatic" => {
                            println!("[track:{}] loading tuner model={} reference_hz={}", track.id.0, model, stage.reference_hz);
                            let (mut tuner, _handle) = ChromaticTuner::new(DEFAULT_SAMPLE_RATE as usize);
                            tuner.set_enabled(true);
                            processors.push(RuntimeProcessor::Tuner(tuner));
                        }
                        other => bail!("track '{}' uses unsupported tuner model '{}'", track.id.0, other),
                    }
                }
                CoreBlockKind::Compressor(stage) => {
                    println!("[track:{}] loading compressor threshold={} ratio={} attack_ms={} release_ms={} makeup_gain_db={} mix={}", track.id.0, stage.threshold, stage.ratio, stage.attack_ms, stage.release_ms, stage.makeup_gain_db, stage.mix);
                    processors.push(RuntimeProcessor::Compressor(Compressor::new(
                        stage.threshold,
                        stage.ratio,
                        stage.attack_ms,
                        stage.release_ms,
                        stage.makeup_gain_db,
                        stage.mix,
                        DEFAULT_SAMPLE_RATE,
                    )));
                }
                CoreBlockKind::Gate(stage) => {
                    println!("[track:{}] loading gate threshold={} attack_ms={} release_ms={}", track.id.0, stage.threshold, stage.attack_ms, stage.release_ms);
                    processors.push(RuntimeProcessor::Gate(NoiseGate::new(
                        stage.threshold,
                        stage.attack_ms,
                        stage.release_ms,
                        DEFAULT_SAMPLE_RATE,
                    )));
                }
                CoreBlockKind::Eq(stage) => {
                    println!("[track:{}] loading eq low_gain_db={} mid_gain_db={} high_gain_db={}", track.id.0, stage.low_gain_db, stage.mid_gain_db, stage.high_gain_db);
                    processors.push(RuntimeProcessor::Eq(ThreeBandEq::new(
                        stage.low_gain_db,
                        stage.mid_gain_db,
                        stage.high_gain_db,
                        DEFAULT_SAMPLE_RATE,
                    )));
                }
                CoreBlockKind::Tremolo(stage) => {
                    println!("[track:{}] loading tremolo rate_hz={} depth={}", track.id.0, stage.rate_hz, stage.depth);
                    processors.push(RuntimeProcessor::Tremolo(Tremolo::new(
                        stage.rate_hz,
                        stage.depth,
                        DEFAULT_SAMPLE_RATE,
                    )));
                }
                _ => {}
            },
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
    let mut tuner_samples = Vec::new();
    let tuner_enabled = locked
        .processors
        .iter()
        .any(|processor| matches!(processor, RuntimeProcessor::Tuner(_)));
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
        if tuner_enabled {
            let sample = if track_frame.len() == 1 {
                track_frame[0]
            } else {
                track_frame.iter().copied().sum::<f32>() / track_frame.len() as f32
            };
            tuner_samples.push(sample);
        }
        locked.queue.push_back(track_frame);
        if locked.queue.len() > DEFAULT_QUEUE_CAPACITY_FRAMES {
            locked.queue.pop_front();
        }
    }
    if tuner_enabled && !tuner_samples.is_empty() {
        for processor in &mut locked.processors {
            if let RuntimeProcessor::Tuner(tuner) = processor {
                tuner.process(&tuner_samples);
            }
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
                RuntimeProcessor::Nam(processor) => sample = processor.process_sample(sample),
                RuntimeProcessor::Delay(processor) => sample = processor.process_sample(sample),
                RuntimeProcessor::Reverb(processor) => sample = processor.process_sample(sample),
                RuntimeProcessor::Compressor(processor) => sample = processor.process_sample(sample),
                RuntimeProcessor::Gate(processor) => sample = processor.process_sample(sample),
                RuntimeProcessor::Eq(processor) => sample = processor.process_sample(sample),
                RuntimeProcessor::Tremolo(processor) => sample = processor.process_sample(sample),
                RuntimeProcessor::Tuner(_) => {}
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
