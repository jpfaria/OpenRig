use anyhow::{anyhow, Result};
use domain::ids::{OutputId, TrackId};
use setup::block::{schema_for_block_model, AudioBlockKind, CoreBlockKind, NamBlock, SelectBlock};
use setup::io::{Input, Output};
use setup::param::ParameterSet;
use setup::setup::Setup;
use setup::track::{Track, TrackOutputMixdown};
use stage_amp_combo::{amp_combo_asset_summary, build_amp_combo_processor_for_layout};
use stage_amp_head::{amp_head_asset_summary, build_amp_head_processor_for_layout};
use stage_core::{
    AudioChannelLayout, ModelAudioMode, MonoProcessor, StageProcessor, StereoProcessor,
};
use stage_delay::build_delay_processor_for_layout;
use stage_dyn::build_compressor_processor_for_layout;
use stage_dyn::build_gate_processor_for_layout;
use stage_filter::build_eq_processor_for_layout;
use stage_full_rig::{build_full_rig_processor_for_layout, full_rig_asset_summary};
use stage_gain::{build_drive_processor_for_layout, drive_asset_summary};
use stage_mod::build_tremolo_processor_for_layout;
use stage_nam::{build_nam_processor_for_layout, GENERIC_NAM_MODEL_ID};
use stage_reverb::build_reverb_processor_for_layout;
use stage_util::{build_tuner_processor, tuner_chromatic::ChromaticTuner};
use std::collections::{HashMap, VecDeque};
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};

const DEBUG_MIN_PEAK_TO_LOG: f32 = 0.01;
const DEBUG_LOG_INTERVAL_MS: u64 = 300;
const DEFAULT_QUEUE_CAPACITY_FRAMES: usize = 48_000;
const DEFAULT_SAMPLE_RATE: f32 = 48_000.0;

#[derive(Debug, Clone, Copy)]
struct QueuedFrame {
    sequence: u64,
    frame: AudioFrame,
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
    DualMono {
        left: Box<dyn MonoProcessor>,
        right: Box<dyn MonoProcessor>,
    },
    Stereo(Box<dyn StereoProcessor>),
    StereoFromMono(Box<dyn StereoProcessor>),
}

impl AudioProcessor {
    fn process_buffer(&mut self, frames: &mut [AudioFrame]) {
        match self {
            AudioProcessor::Mono(processor) => {
                let mut mono = Vec::with_capacity(frames.len());
                for frame in &*frames {
                    match frame {
                        AudioFrame::Mono(sample) => mono.push(*sample),
                        AudioFrame::Stereo(_) => {
                            debug_assert!(false, "mono processor received stereo frames");
                            return;
                        }
                    }
                }
                processor.process_block(&mut mono);
                for (frame, sample) in frames.iter_mut().zip(mono.into_iter()) {
                    *frame = AudioFrame::Mono(sample);
                }
            }
            AudioProcessor::DualMono { left, right } => {
                let mut left_buffer = Vec::with_capacity(frames.len());
                let mut right_buffer = Vec::with_capacity(frames.len());
                for frame in &*frames {
                    match frame {
                        AudioFrame::Stereo([left_sample, right_sample]) => {
                            left_buffer.push(*left_sample);
                            right_buffer.push(*right_sample);
                        }
                        AudioFrame::Mono(_) => {
                            debug_assert!(false, "dual-mono processor received mono frames");
                            return;
                        }
                    }
                }
                left.process_block(&mut left_buffer);
                right.process_block(&mut right_buffer);
                for ((frame, left_sample), right_sample) in frames
                    .iter_mut()
                    .zip(left_buffer.into_iter())
                    .zip(right_buffer.into_iter())
                {
                    *frame = AudioFrame::Stereo([left_sample, right_sample]);
                }
            }
            AudioProcessor::Stereo(processor) => {
                let mut stereo = Vec::with_capacity(frames.len());
                for frame in &*frames {
                    match frame {
                        AudioFrame::Stereo(stereo_frame) => stereo.push(*stereo_frame),
                        AudioFrame::Mono(_) => {
                            debug_assert!(false, "stereo processor received mono frames");
                            return;
                        }
                    }
                }
                processor.process_block(&mut stereo);
                for (frame, stereo_frame) in frames.iter_mut().zip(stereo.into_iter()) {
                    *frame = AudioFrame::Stereo(stereo_frame);
                }
            }
            AudioProcessor::StereoFromMono(processor) => {
                let mut stereo = Vec::with_capacity(frames.len());
                for frame in &*frames {
                    match frame {
                        AudioFrame::Mono(sample) => stereo.push([*sample, *sample]),
                        AudioFrame::Stereo(_) => {
                            debug_assert!(false, "mono-to-stereo processor received stereo frames");
                            return;
                        }
                    }
                }
                processor.process_block(&mut stereo);
                for (frame, stereo_frame) in frames.iter_mut().zip(stereo.into_iter()) {
                    *frame = AudioFrame::Stereo(stereo_frame);
                }
            }
        }
    }
}

pub struct TrackRuntimeState {
    input_layout: AudioChannelLayout,
    output_layout: AudioChannelLayout,
    processed_frames: VecDeque<QueuedFrame>,
    next_sequence: u64,
    output_positions: HashMap<OutputId, u64>,
    last_print: Instant,
    processors: Vec<RuntimeProcessor>,
}

enum RuntimeProcessor {
    Audio(AudioProcessor),
    Tuner(ChromaticTuner),
}

struct ProcessorBuildOutcome {
    processor: AudioProcessor,
    output_layout: AudioChannelLayout,
}

pub struct RuntimeGraph {
    pub tracks: HashMap<TrackId, Arc<Mutex<TrackRuntimeState>>>,
}

pub fn build_runtime_graph(setup: &Setup) -> Result<RuntimeGraph> {
    let mut tracks = HashMap::new();
    for track in &setup.tracks {
        if !track.enabled {
            continue;
        }
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
        let input_layout = layout_from_channels(input_cfg.channels.len())?;
        let (processors, output_layout) = build_runtime_processors(track, input_layout)?;
        println!(
            "[track:{}] runtime input_layout={} output_layout={}",
            track.id.0,
            layout_label(input_layout),
            layout_label(output_layout)
        );
        tracks.insert(
            track.id.clone(),
            Arc::new(Mutex::new(TrackRuntimeState {
                input_layout,
                output_layout,
                processed_frames: VecDeque::with_capacity(DEFAULT_QUEUE_CAPACITY_FRAMES),
                next_sequence: 0,
                output_positions: track
                    .output_ids
                    .iter()
                    .cloned()
                    .map(|output_id| (output_id, 0))
                    .collect(),
                last_print: Instant::now(),
                processors,
            })),
        );
    }
    Ok(RuntimeGraph { tracks })
}

fn build_runtime_processors(
    track: &Track,
    input_layout: AudioChannelLayout,
) -> Result<(Vec<RuntimeProcessor>, AudioChannelLayout)> {
    let mut processors = Vec::new();
    let mut current_layout = input_layout;

    for block in &track.blocks {
        if !block.enabled {
            continue;
        }
        match &block.kind {
            AudioBlockKind::Nam(stage) => {
                let outcome = build_nam_audio_processor(track, stage, current_layout, "nam")?;
                current_layout = outcome.output_layout;
                processors.push(RuntimeProcessor::Audio(outcome.processor));
            }
            AudioBlockKind::Core(core) => match &core.kind {
                CoreBlockKind::AmpHead(stage) => {
                    println!(
                        "[track:{}] loading amp-head model={} {}",
                        track.id.0,
                        stage.model,
                        amp_head_asset_summary(&stage.model, &stage.params)?
                    );
                    let outcome = build_audio_processor_for_model(
                        track,
                        "amp_head",
                        &stage.model,
                        current_layout,
                        |layout| {
                            build_amp_head_processor_for_layout(
                                &stage.model,
                                &stage.params,
                                DEFAULT_SAMPLE_RATE,
                                layout,
                            )
                        },
                    )?;
                    current_layout = outcome.output_layout;
                    processors.push(RuntimeProcessor::Audio(outcome.processor));
                }
                CoreBlockKind::AmpCombo(stage) => {
                    println!(
                        "[track:{}] loading amp-combo model={} {}",
                        track.id.0,
                        stage.model,
                        amp_combo_asset_summary(&stage.model, &stage.params)?
                    );
                    let outcome = build_audio_processor_for_model(
                        track,
                        "amp_combo",
                        &stage.model,
                        current_layout,
                        |layout| {
                            build_amp_combo_processor_for_layout(
                                &stage.model,
                                &stage.params,
                                DEFAULT_SAMPLE_RATE,
                                layout,
                            )
                        },
                    )?;
                    current_layout = outcome.output_layout;
                    processors.push(RuntimeProcessor::Audio(outcome.processor));
                }
                CoreBlockKind::FullRig(stage) => {
                    println!(
                        "[track:{}] loading full-rig model={} {}",
                        track.id.0,
                        stage.model,
                        full_rig_asset_summary(&stage.model, &stage.params)?
                    );
                    let outcome = build_audio_processor_for_model(
                        track,
                        "full_rig",
                        &stage.model,
                        current_layout,
                        |layout| {
                            build_full_rig_processor_for_layout(
                                &stage.model,
                                &stage.params,
                                DEFAULT_SAMPLE_RATE,
                                layout,
                            )
                        },
                    )?;
                    current_layout = outcome.output_layout;
                    processors.push(RuntimeProcessor::Audio(outcome.processor));
                }
                CoreBlockKind::Drive(stage) => {
                    println!(
                        "[track:{}] loading drive model={} {}",
                        track.id.0,
                        stage.model,
                        drive_asset_summary(&stage.model, &stage.params)?
                    );
                    let outcome = build_audio_processor_for_model(
                        track,
                        "drive",
                        &stage.model,
                        current_layout,
                        |layout| {
                            build_drive_processor_for_layout(
                                &stage.model,
                                &stage.params,
                                DEFAULT_SAMPLE_RATE,
                                layout,
                            )
                        },
                    )?;
                    current_layout = outcome.output_layout;
                    processors.push(RuntimeProcessor::Audio(outcome.processor));
                }
                CoreBlockKind::Delay(stage) => {
                    let time_ms = required_f32(&stage.params, "time_ms").unwrap_or_default();
                    let feedback = required_f32(&stage.params, "feedback").unwrap_or_default();
                    let mix = required_f32(&stage.params, "mix").unwrap_or_default();
                    println!(
                        "[track:{}] loading delay model={} time_ms={} feedback={} mix={}",
                        track.id.0, stage.model, time_ms, feedback, mix
                    );
                    let outcome = build_audio_processor_for_model(
                        track,
                        "delay",
                        &stage.model,
                        current_layout,
                        |layout| {
                            build_delay_processor_for_layout(
                                &stage.model,
                                &stage.params,
                                DEFAULT_SAMPLE_RATE,
                                layout,
                            )
                        },
                    )?;
                    current_layout = outcome.output_layout;
                    processors.push(RuntimeProcessor::Audio(outcome.processor));
                }
                CoreBlockKind::Reverb(stage) => {
                    let room_size = required_f32(&stage.params, "room_size").unwrap_or_default();
                    let damping = required_f32(&stage.params, "damping").unwrap_or_default();
                    let mix = required_f32(&stage.params, "mix").unwrap_or_default();
                    println!(
                        "[track:{}] loading reverb model={} room_size={} damping={} mix={}",
                        track.id.0, stage.model, room_size, damping, mix
                    );
                    let outcome = build_audio_processor_for_model(
                        track,
                        "reverb",
                        &stage.model,
                        current_layout,
                        |layout| {
                            build_reverb_processor_for_layout(
                                &stage.model,
                                &stage.params,
                                DEFAULT_SAMPLE_RATE,
                                layout,
                            )
                        },
                    )?;
                    current_layout = outcome.output_layout;
                    processors.push(RuntimeProcessor::Audio(outcome.processor));
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
                    let outcome = build_audio_processor_for_model(
                        track,
                        "compressor",
                        &stage.model,
                        current_layout,
                        |layout| {
                            build_compressor_processor_for_layout(
                                &stage.model,
                                &stage.params,
                                DEFAULT_SAMPLE_RATE,
                                layout,
                            )
                        },
                    )?;
                    current_layout = outcome.output_layout;
                    processors.push(RuntimeProcessor::Audio(outcome.processor));
                }
                CoreBlockKind::Gate(stage) => {
                    let threshold = required_f32(&stage.params, "threshold")?;
                    let attack_ms = required_f32(&stage.params, "attack_ms")?;
                    let release_ms = required_f32(&stage.params, "release_ms")?;
                    println!(
                        "[track:{}] loading gate model={} threshold={} attack_ms={} release_ms={}",
                        track.id.0, stage.model, threshold, attack_ms, release_ms
                    );
                    let outcome = build_audio_processor_for_model(
                        track,
                        "gate",
                        &stage.model,
                        current_layout,
                        |layout| {
                            build_gate_processor_for_layout(
                                &stage.model,
                                &stage.params,
                                DEFAULT_SAMPLE_RATE,
                                layout,
                            )
                        },
                    )?;
                    current_layout = outcome.output_layout;
                    processors.push(RuntimeProcessor::Audio(outcome.processor));
                }
                CoreBlockKind::Eq(stage) => {
                    let low_gain_db = required_f32(&stage.params, "low_gain_db")?;
                    let mid_gain_db = required_f32(&stage.params, "mid_gain_db")?;
                    let high_gain_db = required_f32(&stage.params, "high_gain_db")?;
                    println!(
                        "[track:{}] loading eq model={} low_gain_db={} mid_gain_db={} high_gain_db={}",
                        track.id.0, stage.model, low_gain_db, mid_gain_db, high_gain_db
                    );
                    let outcome = build_audio_processor_for_model(
                        track,
                        "eq",
                        &stage.model,
                        current_layout,
                        |layout| {
                            build_eq_processor_for_layout(
                                &stage.model,
                                &stage.params,
                                DEFAULT_SAMPLE_RATE,
                                layout,
                            )
                        },
                    )?;
                    current_layout = outcome.output_layout;
                    processors.push(RuntimeProcessor::Audio(outcome.processor));
                }
                CoreBlockKind::Tremolo(stage) => {
                    let rate_hz = required_f32(&stage.params, "rate_hz")?;
                    let depth = required_f32(&stage.params, "depth")?;
                    println!(
                        "[track:{}] loading tremolo model={} rate_hz={} depth={}",
                        track.id.0, stage.model, rate_hz, depth
                    );
                    let outcome = build_audio_processor_for_model(
                        track,
                        "tremolo",
                        &stage.model,
                        current_layout,
                        |layout| {
                            build_tremolo_processor_for_layout(
                                &stage.model,
                                &stage.params,
                                DEFAULT_SAMPLE_RATE,
                                layout,
                            )
                        },
                    )?;
                    current_layout = outcome.output_layout;
                    processors.push(RuntimeProcessor::Audio(outcome.processor));
                }
                _ => {}
            },
            AudioBlockKind::Select(select) => {
                let outcome = load_selected_nam(track, select, current_layout)?;
                current_layout = outcome.output_layout;
                processors.push(RuntimeProcessor::Audio(outcome.processor));
            }
            _ => {}
        }
    }

    Ok((processors, current_layout))
}

fn build_audio_processor_for_model<F>(
    track: &Track,
    effect_type: &str,
    model: &str,
    input_layout: AudioChannelLayout,
    mut builder: F,
) -> Result<ProcessorBuildOutcome>
where
    F: FnMut(AudioChannelLayout) -> Result<StageProcessor>,
{
    let schema = schema_for_block_model(effect_type, model).map_err(|error| {
        anyhow!(
            "track '{}' {} model '{}': {}",
            track.id.0,
            effect_type,
            model,
            error
        )
    })?;

    let output_layout = schema
        .audio_mode
        .output_layout(input_layout)
        .ok_or_else(|| {
            anyhow!(
                "track '{}' {} model '{}' with audio mode '{}' does not accept {} input",
                track.id.0,
                effect_type,
                model,
                schema.audio_mode.as_str(),
                layout_label(input_layout)
            )
        })?;

    let processor = match (schema.audio_mode, input_layout) {
        (ModelAudioMode::MonoOnly, AudioChannelLayout::Mono) => {
            AudioProcessor::Mono(expect_mono_processor(
                builder(AudioChannelLayout::Mono)?,
                track,
                effect_type,
                model,
            )?)
        }
        (ModelAudioMode::DualMono, AudioChannelLayout::Mono) => {
            AudioProcessor::Mono(expect_mono_processor(
                builder(AudioChannelLayout::Mono)?,
                track,
                effect_type,
                model,
            )?)
        }
        (ModelAudioMode::DualMono, AudioChannelLayout::Stereo) => AudioProcessor::DualMono {
            left: expect_mono_processor(
                builder(AudioChannelLayout::Mono)?,
                track,
                effect_type,
                model,
            )?,
            right: expect_mono_processor(
                builder(AudioChannelLayout::Mono)?,
                track,
                effect_type,
                model,
            )?,
        },
        (ModelAudioMode::TrueStereo, AudioChannelLayout::Stereo) => {
            AudioProcessor::Stereo(expect_stereo_processor(
                builder(AudioChannelLayout::Stereo)?,
                track,
                effect_type,
                model,
            )?)
        }
        (ModelAudioMode::MonoToStereo, AudioChannelLayout::Mono) => {
            AudioProcessor::StereoFromMono(expect_stereo_processor(
                builder(AudioChannelLayout::Stereo)?,
                track,
                effect_type,
                model,
            )?)
        }
        (ModelAudioMode::MonoToStereo, AudioChannelLayout::Stereo) => {
            AudioProcessor::Stereo(expect_stereo_processor(
                builder(AudioChannelLayout::Stereo)?,
                track,
                effect_type,
                model,
            )?)
        }
        _ => {
            return Err(anyhow!(
                "track '{}' {} model '{}' with audio mode '{}' cannot run on {} input",
                track.id.0,
                effect_type,
                model,
                schema.audio_mode.as_str(),
                layout_label(input_layout)
            ));
        }
    };

    println!(
        "[track:{}] {} model={} audio_mode={} input_layout={} output_layout={} runtime_mode={}",
        track.id.0,
        effect_type,
        model,
        schema.audio_mode.as_str(),
        layout_label(input_layout),
        layout_label(output_layout),
        audio_processor_runtime_mode(&processor)
    );

    Ok(ProcessorBuildOutcome {
        processor,
        output_layout,
    })
}

fn build_nam_audio_processor(
    track: &Track,
    stage: &NamBlock,
    input_layout: AudioChannelLayout,
    label: &str,
) -> Result<ProcessorBuildOutcome> {
    if stage.model != GENERIC_NAM_MODEL_ID {
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
    build_audio_processor_for_model(track, "nam", &stage.model, input_layout, |layout| {
        build_nam_processor_for_layout(&stage.model, &stage.params, layout)
    })
}

fn expect_mono_processor(
    processor: StageProcessor,
    track: &Track,
    effect_type: &str,
    model: &str,
) -> Result<Box<dyn MonoProcessor>> {
    match processor {
        StageProcessor::Mono(processor) => Ok(processor),
        StageProcessor::Stereo(_) => Err(anyhow!(
            "track '{}' {} model '{}' returned stereo processing where mono was required",
            track.id.0,
            effect_type,
            model
        )),
    }
}

fn expect_stereo_processor(
    processor: StageProcessor,
    track: &Track,
    effect_type: &str,
    model: &str,
) -> Result<Box<dyn StereoProcessor>> {
    match processor {
        StageProcessor::Stereo(processor) => Ok(processor),
        StageProcessor::Mono(_) => Err(anyhow!(
            "track '{}' {} model '{}' returned mono processing where stereo was required",
            track.id.0,
            effect_type,
            model
        )),
    }
}

fn audio_processor_runtime_mode(processor: &AudioProcessor) -> &'static str {
    match processor {
        AudioProcessor::Mono(_) => "mono",
        AudioProcessor::DualMono { .. } => "dual_mono",
        AudioProcessor::Stereo(_) => "stereo",
        AudioProcessor::StereoFromMono(_) => "mono_to_stereo",
    }
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
    input_layout: AudioChannelLayout,
) -> Result<ProcessorBuildOutcome> {
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
            build_nam_audio_processor(track, stage, input_layout, "selected NAM")
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
    let mut track_frames = Vec::with_capacity(data.len() / input_total_channels);
    let mut tuner_samples = Vec::new();
    let tuner_enabled = locked
        .processors
        .iter()
        .any(|processor| matches!(processor, RuntimeProcessor::Tuner(_)));

    for frame in data.chunks(input_total_channels) {
        let track_frame = read_input_frame(locked.input_layout, input_cfg, frame);
        peak = peak.max(track_frame.peak());
        if tuner_enabled {
            tuner_samples.push(track_frame.mono_mix());
        }
        track_frames.push(track_frame);
    }

    if tuner_enabled && !tuner_samples.is_empty() {
        for processor in &mut locked.processors {
            if let RuntimeProcessor::Tuner(tuner) = processor {
                tuner.process(&tuner_samples);
            }
        }
    }

    for processor in &mut locked.processors {
        if let RuntimeProcessor::Audio(processor) = processor {
            processor.process_buffer(&mut track_frames);
        }
    }

    for frame in track_frames {
        let sequence = locked.next_sequence;
        locked.next_sequence += 1;
        locked
            .processed_frames
            .push_back(QueuedFrame { sequence, frame });
    }
    trim_processed_frames(&mut locked);

    if peak >= DEBUG_MIN_PEAK_TO_LOG
        && locked.last_print.elapsed() >= Duration::from_millis(DEBUG_LOG_INTERVAL_MS)
    {
        println!(
            "[{}] audio detected | input_channels={:?} | peak={:.4} | buffered={}",
            track.id.0,
            input_cfg.channels,
            peak,
            locked.processed_frames.len()
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
    let num_frames = out.len() / output_total_channels;
    let next_sequence = locked.next_sequence;
    let mut cursor = *locked
        .output_positions
        .entry(output_cfg.id.clone())
        .or_insert(next_sequence);

    if let Some(oldest_sequence) = locked.processed_frames.front().map(|frame| frame.sequence) {
        if cursor < oldest_sequence {
            cursor = oldest_sequence;
        }
    }

    for frame in out.chunks_mut(output_total_channels).take(num_frames) {
        frame.fill(0.0);
        let mut processed = queued_frame_at(&locked.processed_frames, cursor)
            .map(|frame| frame.frame)
            .unwrap_or_else(|| silent_frame(locked.output_layout));
        processed.apply_gain(track.gain);
        write_output_frame(processed, output_cfg, frame, track.output_mixdown);
        cursor += 1;
    }

    locked
        .output_positions
        .insert(output_cfg.id.clone(), cursor);
    trim_processed_frames(&mut locked);
}

fn queued_frame_at(queue: &VecDeque<QueuedFrame>, sequence: u64) -> Option<QueuedFrame> {
    let oldest_sequence = queue.front()?.sequence;
    let index = sequence.checked_sub(oldest_sequence)? as usize;
    queue.get(index).copied()
}

fn trim_processed_frames(state: &mut TrackRuntimeState) {
    let min_cursor = state
        .output_positions
        .values()
        .copied()
        .min()
        .unwrap_or(state.next_sequence);

    while let Some(front) = state.processed_frames.front() {
        if front.sequence < min_cursor {
            state.processed_frames.pop_front();
        } else {
            break;
        }
    }

    while state.processed_frames.len() > DEFAULT_QUEUE_CAPACITY_FRAMES {
        state.processed_frames.pop_front();
    }

    if let Some(oldest_sequence) = state.processed_frames.front().map(|frame| frame.sequence) {
        for cursor in state.output_positions.values_mut() {
            if *cursor < oldest_sequence {
                *cursor = oldest_sequence;
            }
        }
    } else {
        for cursor in state.output_positions.values_mut() {
            *cursor = state.next_sequence;
        }
    }
}

fn read_input_frame(
    input_layout: AudioChannelLayout,
    input_cfg: &Input,
    frame: &[f32],
) -> AudioFrame {
    match input_layout {
        AudioChannelLayout::Mono => AudioFrame::Mono(read_channel(frame, input_cfg.channels[0])),
        AudioChannelLayout::Stereo => AudioFrame::Stereo([
            read_channel(frame, input_cfg.channels[0]),
            read_channel(frame, input_cfg.channels[1]),
        ]),
    }
}

fn read_channel(frame: &[f32], channel_index: usize) -> f32 {
    frame.get(channel_index).copied().unwrap_or(0.0)
}

fn silent_frame(layout: AudioChannelLayout) -> AudioFrame {
    match layout {
        AudioChannelLayout::Mono => AudioFrame::Mono(0.0),
        AudioChannelLayout::Stereo => AudioFrame::Stereo([0.0, 0.0]),
    }
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

fn layout_from_channels(channel_count: usize) -> Result<AudioChannelLayout> {
    match channel_count {
        1 => Ok(AudioChannelLayout::Mono),
        2 => Ok(AudioChannelLayout::Stereo),
        other => Err(anyhow!(
            "only mono and stereo are supported right now; got {} channels",
            other
        )),
    }
}

fn layout_label(layout: AudioChannelLayout) -> &'static str {
    match layout {
        AudioChannelLayout::Mono => "mono",
        AudioChannelLayout::Stereo => "stereo",
    }
}
