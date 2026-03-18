use anyhow::{anyhow, Result};
use domain::ids::TrackId;
use setup::block::{AudioBlockKind, CoreBlockKind, NamBlock, SelectBlock};
use setup::io::{Input, Output};
use setup::param::ParameterSet;
use setup::setup::Setup;
use setup::track::{Track, TrackBusMode, TrackOutputMixdown};
use stage_amp_nam::{build_nam_processor, processor::DEFAULT_NAM_MODEL};
use stage_core::MonoProcessor;
use stage_delay_digital::build_delay_processor;
use stage_dyn_compressor::build_compressor_processor;
use stage_dyn_gate::build_gate_processor;
use stage_filter_eq::build_eq_processor;
use stage_mod_tremolo::build_tremolo_processor;
use stage_reverb_plate::build_reverb_processor;
use stage_util_tuner::{build_tuner_processor, chromatic::ChromaticTuner};
use std::collections::{HashMap, VecDeque};
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};

const DEBUG_MIN_PEAK_TO_LOG: f32 = 0.01;
const DEBUG_LOG_INTERVAL_MS: u64 = 300;
const DEFAULT_QUEUE_CAPACITY_FRAMES: usize = 48_000;
const DEFAULT_SAMPLE_RATE: f32 = 48_000.0;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum ResolvedTrackBusMode {
    Mono,
    Stereo,
}

#[derive(Debug, Clone, Copy)]
enum AudioFrame {
    Mono(f32),
    Stereo([f32; 2]),
}

impl AudioFrame {
    fn mono_mix(self) -> f32 {
        match self {
            AudioFrame::Mono(sample) => sample,
            AudioFrame::Stereo([left, right]) => (left + right) * 0.5,
        }
    }

    fn peak(self) -> f32 {
        match self {
            AudioFrame::Mono(sample) => sample.abs(),
            AudioFrame::Stereo([left, right]) => left.abs().max(right.abs()),
        }
    }

    fn apply_gain(&mut self, gain: f32) {
        match self {
            AudioFrame::Mono(sample) => *sample *= gain,
            AudioFrame::Stereo([left, right]) => {
                *left *= gain;
                *right *= gain;
            }
        }
    }
}

enum AudioProcessor {
    Mono(Box<dyn MonoProcessor>),
    Stereo {
        left: Box<dyn MonoProcessor>,
        right: Box<dyn MonoProcessor>,
    },
}

impl AudioProcessor {
    fn process_frame(&mut self, frame: AudioFrame) -> AudioFrame {
        match self {
            AudioProcessor::Mono(processor) => match frame {
                AudioFrame::Mono(sample) => AudioFrame::Mono(processor.process_sample(sample)),
                AudioFrame::Stereo(stereo) => {
                    debug_assert!(false, "mono processor received stereo frame");
                    AudioFrame::Stereo(stereo)
                }
            },
            AudioProcessor::Stereo { left, right } => match frame {
                AudioFrame::Stereo([left_sample, right_sample]) => AudioFrame::Stereo([
                    left.process_sample(left_sample),
                    right.process_sample(right_sample),
                ]),
                AudioFrame::Mono(sample) => {
                    debug_assert!(false, "stereo processor received mono frame");
                    AudioFrame::Mono(sample)
                }
            },
        }
    }
}

pub struct TrackRuntimeState {
    bus_mode: ResolvedTrackBusMode,
    queue: VecDeque<AudioFrame>,
    last_print: Instant,
    processors: Vec<RuntimeProcessor>,
}

enum RuntimeProcessor {
    Audio(AudioProcessor),
    Tuner(ChromaticTuner),
}

pub struct RuntimeGraph {
    pub tracks: HashMap<TrackId, Arc<Mutex<TrackRuntimeState>>>,
}

pub fn build_runtime_graph(setup: &Setup) -> Result<RuntimeGraph> {
    let mut tracks = HashMap::new();
    for track in &setup.tracks {
        let input_cfg = setup
            .inputs
            .iter()
            .find(|input| input.id == track.input_id)
            .ok_or_else(|| {
                anyhow!(
                    "track '{}' references missing input '{}'",
                    track.id.0,
                    track.input_id.0
                )
            })?;
        let bus_mode = resolve_track_bus_mode(track, input_cfg);
        println!(
            "[track:{}] runtime bus_mode={}",
            track.id.0,
            match bus_mode {
                ResolvedTrackBusMode::Mono => "mono",
                ResolvedTrackBusMode::Stereo => "stereo",
            }
        );
        tracks.insert(
            track.id.clone(),
            Arc::new(Mutex::new(TrackRuntimeState {
                bus_mode,
                queue: VecDeque::with_capacity(DEFAULT_QUEUE_CAPACITY_FRAMES),
                last_print: Instant::now(),
                processors: build_runtime_processors(track, bus_mode)?,
            })),
        );
    }
    Ok(RuntimeGraph { tracks })
}

fn resolve_track_bus_mode(track: &Track, input_cfg: &Input) -> ResolvedTrackBusMode {
    match track.bus_mode {
        TrackBusMode::Auto => {
            if input_cfg.channels.len() >= 2 {
                ResolvedTrackBusMode::Stereo
            } else {
                ResolvedTrackBusMode::Mono
            }
        }
        TrackBusMode::Mono => ResolvedTrackBusMode::Mono,
        TrackBusMode::Stereo => ResolvedTrackBusMode::Stereo,
    }
}

fn build_runtime_processors(
    track: &Track,
    bus_mode: ResolvedTrackBusMode,
) -> Result<Vec<RuntimeProcessor>> {
    let mut processors = Vec::new();
    for block in &track.blocks {
        match &block.kind {
            AudioBlockKind::Nam(stage) => {
                processors.push(RuntimeProcessor::Audio(build_nam_audio_processor(
                    track, stage, bus_mode, "nam",
                )?));
            }
            AudioBlockKind::Core(core) => match &core.kind {
                CoreBlockKind::Delay(stage) => {
                    let time_ms = required_f32(&stage.params, "time_ms")?;
                    let feedback = required_f32(&stage.params, "feedback")?;
                    let mix = required_f32(&stage.params, "mix")?;
                    println!(
                        "[track:{}] loading delay model={} time_ms={} feedback={} mix={}",
                        track.id.0, stage.model, time_ms, feedback, mix
                    );
                    processors.push(RuntimeProcessor::Audio(build_audio_processor(
                        bus_mode,
                        || build_delay_processor(&stage.model, &stage.params, DEFAULT_SAMPLE_RATE),
                    )?));
                }
                CoreBlockKind::Reverb(stage) => {
                    let room_size = required_f32(&stage.params, "room_size")?;
                    let damping = required_f32(&stage.params, "damping")?;
                    let mix = required_f32(&stage.params, "mix")?;
                    println!(
                        "[track:{}] loading reverb model={} room_size={} damping={} mix={}",
                        track.id.0, stage.model, room_size, damping, mix
                    );
                    processors.push(RuntimeProcessor::Audio(build_audio_processor(
                        bus_mode,
                        || build_reverb_processor(&stage.model, &stage.params, DEFAULT_SAMPLE_RATE),
                    )?));
                }
                CoreBlockKind::Tuner(stage) => {
                    let reference_hz = required_f32(&stage.params, "reference_hz")?;
                    println!(
                        "[track:{}] loading tuner model={} reference_hz={}",
                        track.id.0, stage.model, reference_hz
                    );
                    processors.push(RuntimeProcessor::Tuner(build_tuner_processor(
                        &stage.model,
                        &stage.params,
                        DEFAULT_SAMPLE_RATE as usize,
                    )?));
                }
                CoreBlockKind::Compressor(stage) => {
                    let threshold = required_f32(&stage.params, "threshold")?;
                    let ratio = required_f32(&stage.params, "ratio")?;
                    let attack_ms = required_f32(&stage.params, "attack_ms")?;
                    let release_ms = required_f32(&stage.params, "release_ms")?;
                    let makeup_gain_db = required_f32(&stage.params, "makeup_gain_db")?;
                    let mix = required_f32(&stage.params, "mix")?;
                    println!(
                        "[track:{}] loading compressor model={} threshold={} ratio={} attack_ms={} release_ms={} makeup_gain_db={} mix={}",
                        track.id.0,
                        stage.model,
                        threshold,
                        ratio,
                        attack_ms,
                        release_ms,
                        makeup_gain_db,
                        mix
                    );
                    processors.push(RuntimeProcessor::Audio(build_audio_processor(
                        bus_mode,
                        || {
                            build_compressor_processor(
                                &stage.model,
                                &stage.params,
                                DEFAULT_SAMPLE_RATE,
                            )
                        },
                    )?));
                }
                CoreBlockKind::Gate(stage) => {
                    let threshold = required_f32(&stage.params, "threshold")?;
                    let attack_ms = required_f32(&stage.params, "attack_ms")?;
                    let release_ms = required_f32(&stage.params, "release_ms")?;
                    println!(
                        "[track:{}] loading gate model={} threshold={} attack_ms={} release_ms={}",
                        track.id.0, stage.model, threshold, attack_ms, release_ms
                    );
                    processors.push(RuntimeProcessor::Audio(build_audio_processor(
                        bus_mode,
                        || build_gate_processor(&stage.model, &stage.params, DEFAULT_SAMPLE_RATE),
                    )?));
                }
                CoreBlockKind::Eq(stage) => {
                    let low_gain_db = required_f32(&stage.params, "low_gain_db")?;
                    let mid_gain_db = required_f32(&stage.params, "mid_gain_db")?;
                    let high_gain_db = required_f32(&stage.params, "high_gain_db")?;
                    println!(
                        "[track:{}] loading eq model={} low_gain_db={} mid_gain_db={} high_gain_db={}",
                        track.id.0, stage.model, low_gain_db, mid_gain_db, high_gain_db
                    );
                    processors.push(RuntimeProcessor::Audio(build_audio_processor(
                        bus_mode,
                        || build_eq_processor(&stage.model, &stage.params, DEFAULT_SAMPLE_RATE),
                    )?));
                }
                CoreBlockKind::Tremolo(stage) => {
                    let rate_hz = required_f32(&stage.params, "rate_hz")?;
                    let depth = required_f32(&stage.params, "depth")?;
                    println!(
                        "[track:{}] loading tremolo model={} rate_hz={} depth={}",
                        track.id.0, stage.model, rate_hz, depth
                    );
                    processors.push(RuntimeProcessor::Audio(build_audio_processor(
                        bus_mode,
                        || {
                            build_tremolo_processor(
                                &stage.model,
                                &stage.params,
                                DEFAULT_SAMPLE_RATE,
                            )
                        },
                    )?));
                }
                _ => {}
            },
            AudioBlockKind::Select(select) => {
                processors.push(RuntimeProcessor::Audio(load_selected_nam(
                    track, select, bus_mode,
                )?));
            }
            _ => {}
        }
    }
    Ok(processors)
}

fn build_audio_processor<F>(
    bus_mode: ResolvedTrackBusMode,
    mut builder: F,
) -> Result<AudioProcessor>
where
    F: FnMut() -> Result<Box<dyn MonoProcessor>>,
{
    Ok(match bus_mode {
        ResolvedTrackBusMode::Mono => AudioProcessor::Mono(builder()?),
        ResolvedTrackBusMode::Stereo => AudioProcessor::Stereo {
            left: builder()?,
            right: builder()?,
        },
    })
}

fn build_nam_audio_processor(
    track: &Track,
    stage: &NamBlock,
    bus_mode: ResolvedTrackBusMode,
    label: &str,
) -> Result<AudioProcessor> {
    if stage.model != DEFAULT_NAM_MODEL {
        return Err(anyhow!(
            "track '{}' uses unsupported nam model '{}'",
            track.id.0,
            stage.model
        ));
    }
    let ir_path = optional_string(&stage.params, "ir_path");
    let model_path = required_string(&stage.params, "model_path")?;
    println!(
        "[track:{}] loading {} model={} file='{}'",
        track.id.0, label, stage.model, model_path
    );
    if let Some(ir_path) = ir_path.as_deref() {
        println!("[track:{}] loading {} IR '{}'", track.id.0, label, ir_path);
    }
    build_audio_processor(bus_mode, || {
        build_nam_processor(&stage.model, &stage.params)
    })
}

fn required_f32(params: &ParameterSet, path: &str) -> Result<f32> {
    params
        .get_f32(path)
        .ok_or_else(|| anyhow!("missing or invalid float parameter '{}'", path))
}

fn required_string(params: &ParameterSet, path: &str) -> Result<String> {
    params
        .get_string(path)
        .map(ToString::to_string)
        .ok_or_else(|| anyhow!("missing or invalid string parameter '{}'", path))
}

fn optional_string(params: &ParameterSet, path: &str) -> Option<String> {
    params
        .get_optional_string(path)
        .flatten()
        .map(ToString::to_string)
}

fn load_selected_nam(
    track: &Track,
    select: &SelectBlock,
    bus_mode: ResolvedTrackBusMode,
) -> Result<AudioProcessor> {
    let selected = select
        .options
        .iter()
        .find(|option| option.id == select.selected_block_id)
        .ok_or_else(|| {
            anyhow!(
                "track '{}' select block references unknown option",
                track.id.0
            )
        })?;

    match &selected.kind {
        AudioBlockKind::Nam(stage) => {
            build_nam_audio_processor(track, stage, bus_mode, "selected NAM")
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
        let track_frame = read_input_frame(locked.bus_mode, input_cfg, frame);
        peak = peak.max(track_frame.peak());

        if tuner_enabled {
            tuner_samples.push(track_frame.mono_mix());
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
        let Some(mut track_frame) = locked.queue.pop_front() else {
            continue;
        };

        for processor in &mut locked.processors {
            match processor {
                RuntimeProcessor::Audio(processor) => {
                    track_frame = processor.process_frame(track_frame);
                }
                RuntimeProcessor::Tuner(_) => {}
            }
        }

        track_frame.apply_gain(track.gain);
        write_output_frame(track_frame, output_cfg, frame, track.output_mixdown);
    }
}

fn read_input_frame(
    bus_mode: ResolvedTrackBusMode,
    input_cfg: &Input,
    frame: &[f32],
) -> AudioFrame {
    match bus_mode {
        ResolvedTrackBusMode::Mono => {
            let sample = if input_cfg.channels.len() == 1 {
                read_channel(frame, input_cfg.channels[0])
            } else {
                let sum = input_cfg
                    .channels
                    .iter()
                    .map(|&channel_index| read_channel(frame, channel_index))
                    .sum::<f32>();
                sum / input_cfg.channels.len() as f32
            };
            AudioFrame::Mono(sample)
        }
        ResolvedTrackBusMode::Stereo => {
            let left = read_channel(frame, input_cfg.channels[0]);
            let right = input_cfg
                .channels
                .get(1)
                .map(|&channel_index| read_channel(frame, channel_index))
                .unwrap_or(left);
            AudioFrame::Stereo([left, right])
        }
    }
}

fn read_channel(frame: &[f32], channel_index: usize) -> f32 {
    frame.get(channel_index).copied().unwrap_or(0.0)
}

fn write_output_frame(
    track_frame: AudioFrame,
    output_cfg: &Output,
    frame: &mut [f32],
    mixdown: TrackOutputMixdown,
) {
    match track_frame {
        AudioFrame::Mono(sample) => {
            for &channel_index in &output_cfg.channels {
                if let Some(dst) = frame.get_mut(channel_index) {
                    *dst = sample;
                }
            }
        }
        AudioFrame::Stereo([left, right]) => match output_cfg.channels.as_slice() {
            [] => {}
            [channel_index] => {
                if let Some(dst) = frame.get_mut(*channel_index) {
                    *dst = apply_mixdown(mixdown, left, right);
                }
            }
            [left_channel, right_channel, ..] => {
                if let Some(dst) = frame.get_mut(*left_channel) {
                    *dst = left;
                }
                if let Some(dst) = frame.get_mut(*right_channel) {
                    *dst = right;
                }
            }
        },
    }
}

fn apply_mixdown(mixdown: TrackOutputMixdown, left: f32, right: f32) -> f32 {
    match mixdown {
        TrackOutputMixdown::Sum => left + right,
        TrackOutputMixdown::Average => (left + right) * 0.5,
        TrackOutputMixdown::Left => left,
        TrackOutputMixdown::Right => right,
    }
}
