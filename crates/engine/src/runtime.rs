use anyhow::{anyhow, Result};
use domain::ids::TrackId;
use setup::block::{AudioBlockKind, CoreBlockKind, SelectBlock};
use setup::io::{Input, Output};
use setup::setup::Setup;
use setup::track::Track;
use stage_core::MonoProcessor;
use stage_amp_nam::processor::{NamPluginParams, NamProcessor, DEFAULT_NAM_MODEL};
use stage_delay_digital::{build_delay_processor, DelayParams};
use stage_dyn_compressor::{build_compressor_processor, CompressorParams};
use stage_dyn_gate::{build_gate_processor, GateParams};
use stage_filter_eq::{build_eq_processor, EqParams};
use stage_mod_tremolo::{build_tremolo_processor, TremoloParams};
use stage_reverb_plate::{build_reverb_processor, ReverbParams};
use stage_util_tuner::{build_tuner_processor, chromatic::ChromaticTuner};
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
    Audio(Box<dyn MonoProcessor>),
    Tuner(ChromaticTuner),
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
                if stage.model != DEFAULT_NAM_MODEL {
                    return Err(anyhow!(
                        "track '{}' uses unsupported nam model '{}'",
                        track.id.0,
                        stage.model
                    ));
                }
                println!(
                    "[track:{}] loading nam model={} file='{}'",
                    track.id.0, stage.model, stage.params.model_path
                );
                if let Some(ir_path) = &stage.params.ir_path {
                    println!("[track:{}] loading NAM IR '{}'", track.id.0, ir_path);
                }
                processors.push(RuntimeProcessor::Audio(Box::new(NamProcessor::new(
                    &stage.params.model_path,
                    stage.params.ir_path.as_deref(),
                    NamPluginParams {
                        input_level_db: stage.params.input_db,
                        output_level_db: stage.params.output_db,
                        noise_gate_threshold_db: stage.params.noise_gate.threshold_db,
                        noise_gate_enabled: stage.params.noise_gate.enabled,
                        eq_enabled: stage.params.eq.enabled,
                        ir_enabled: stage.params.ir_enabled,
                        bass: stage.params.eq.bass,
                        middle: stage.params.eq.middle,
                        treble: stage.params.eq.treble,
                    },
                )?)));
            }
            AudioBlockKind::Core(core) => match &core.kind {
                CoreBlockKind::Delay(stage) => {
                    println!(
                        "[track:{}] loading delay model={} time_ms={} feedback={} mix={}",
                        track.id.0,
                        stage.model,
                        stage.params.time_ms,
                        stage.params.feedback,
                        stage.params.mix
                    );
                    processors.push(RuntimeProcessor::Audio(build_delay_processor(
                        &stage.model,
                        DelayParams {
                            time_ms: stage.params.time_ms,
                            feedback: stage.params.feedback,
                            mix: stage.params.mix,
                        },
                        DEFAULT_SAMPLE_RATE,
                    )?));
                }
                CoreBlockKind::Reverb(stage) => {
                    println!(
                        "[track:{}] loading reverb model={} room_size={} damping={} mix={}",
                        track.id.0,
                        stage.model,
                        stage.params.room_size,
                        stage.params.damping,
                        stage.params.mix
                    );
                    processors.push(RuntimeProcessor::Audio(build_reverb_processor(
                        &stage.model,
                        ReverbParams {
                            room_size: stage.params.room_size,
                            damping: stage.params.damping,
                            mix: stage.params.mix,
                        },
                        DEFAULT_SAMPLE_RATE,
                    )?));
                }
                CoreBlockKind::Tuner(stage) => {
                    println!(
                        "[track:{}] loading tuner model={} reference_hz={}",
                        track.id.0, stage.model, stage.params.reference_hz
                    );
                    processors.push(RuntimeProcessor::Tuner(build_tuner_processor(
                        &stage.model,
                        stage.params.reference_hz,
                        DEFAULT_SAMPLE_RATE as usize,
                    )?));
                }
                CoreBlockKind::Compressor(stage) => {
                    println!(
                        "[track:{}] loading compressor model={} threshold={} ratio={} attack_ms={} release_ms={} makeup_gain_db={} mix={}",
                        track.id.0,
                        stage.model,
                        stage.params.threshold,
                        stage.params.ratio,
                        stage.params.attack_ms,
                        stage.params.release_ms,
                        stage.params.makeup_gain_db,
                        stage.params.mix
                    );
                    processors.push(RuntimeProcessor::Audio(build_compressor_processor(
                        &stage.model,
                        CompressorParams {
                            threshold: stage.params.threshold,
                            ratio: stage.params.ratio,
                            attack_ms: stage.params.attack_ms,
                            release_ms: stage.params.release_ms,
                            makeup_gain_db: stage.params.makeup_gain_db,
                            mix: stage.params.mix,
                        },
                        DEFAULT_SAMPLE_RATE,
                    )?));
                }
                CoreBlockKind::Gate(stage) => {
                    println!(
                        "[track:{}] loading gate model={} threshold={} attack_ms={} release_ms={}",
                        track.id.0,
                        stage.model,
                        stage.params.threshold,
                        stage.params.attack_ms,
                        stage.params.release_ms
                    );
                    processors.push(RuntimeProcessor::Audio(build_gate_processor(
                        &stage.model,
                        GateParams {
                            threshold: stage.params.threshold,
                            attack_ms: stage.params.attack_ms,
                            release_ms: stage.params.release_ms,
                        },
                        DEFAULT_SAMPLE_RATE,
                    )?));
                }
                CoreBlockKind::Eq(stage) => {
                    println!(
                        "[track:{}] loading eq model={} low_gain_db={} mid_gain_db={} high_gain_db={}",
                        track.id.0,
                        stage.model,
                        stage.params.low_gain_db,
                        stage.params.mid_gain_db,
                        stage.params.high_gain_db
                    );
                    processors.push(RuntimeProcessor::Audio(build_eq_processor(
                        &stage.model,
                        EqParams {
                            low_gain_db: stage.params.low_gain_db,
                            mid_gain_db: stage.params.mid_gain_db,
                            high_gain_db: stage.params.high_gain_db,
                        },
                        DEFAULT_SAMPLE_RATE,
                    )?));
                }
                CoreBlockKind::Tremolo(stage) => {
                    println!(
                        "[track:{}] loading tremolo model={} rate_hz={} depth={}",
                        track.id.0, stage.model, stage.params.rate_hz, stage.params.depth
                    );
                    processors.push(RuntimeProcessor::Audio(build_tremolo_processor(
                        &stage.model,
                        TremoloParams {
                            rate_hz: stage.params.rate_hz,
                            depth: stage.params.depth,
                        },
                        DEFAULT_SAMPLE_RATE,
                    )?));
                }
                _ => {}
            },
            AudioBlockKind::Select(select) => {
                processors.push(RuntimeProcessor::Audio(load_selected_nam(track, select)?));
            }
            _ => {}
        }
    }
    Ok(processors)
}

fn load_selected_nam(track: &Track, select: &SelectBlock) -> Result<Box<dyn MonoProcessor>> {
    let selected = select
        .options
        .iter()
        .find(|option| option.id == select.selected_block_id)
        .ok_or_else(|| anyhow!("track '{}' select block references unknown option", track.id.0))?;

    match &selected.kind {
        AudioBlockKind::Nam(stage) => {
            if stage.model != DEFAULT_NAM_MODEL {
                return Err(anyhow!(
                    "track '{}' select block uses unsupported nam model '{}'",
                    track.id.0,
                    stage.model
                ));
            }
            println!(
                "[track:{}] loading selected NAM '{}'",
                track.id.0, stage.params.model_path
            );
            if let Some(ir_path) = &stage.params.ir_path {
                println!("[track:{}] loading selected NAM IR '{}'", track.id.0, ir_path);
            }
            Ok(Box::new(NamProcessor::new(
                &stage.params.model_path,
                stage.params.ir_path.as_deref(),
                NamPluginParams {
                    input_level_db: stage.params.input_db,
                    output_level_db: stage.params.output_db,
                    noise_gate_threshold_db: stage.params.noise_gate.threshold_db,
                    noise_gate_enabled: stage.params.noise_gate.enabled,
                    eq_enabled: stage.params.eq.enabled,
                    ir_enabled: stage.params.ir_enabled,
                    bass: stage.params.eq.bass,
                    middle: stage.params.eq.middle,
                    treble: stage.params.eq.treble,
                },
            )?))
        }
        other => Err(anyhow!(
            "track '{}' select block chose unsupported option: {:?}",
            track.id.0,
            other
        )),
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
                RuntimeProcessor::Audio(processor) => sample = processor.process_sample(sample),
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
