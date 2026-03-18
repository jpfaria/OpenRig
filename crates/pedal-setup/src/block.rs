#![allow(dead_code)]

use pedal-domain::ids::BlockId;
use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct AudioBlock {
    pub id: BlockId,
    pub kind: AudioBlockKind,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub enum AudioBlockKind {
    Nam(NamBlock),
    CoreNam(CoreNamBlock),
    Core(CoreBlock),
    Select(SelectBlock),
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct NamBlock {
    pub model_path: String,
    pub ir_path: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct CoreNamBlock {
    pub model_id: String,
    pub ir_id: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct CoreBlock {
    pub kind: CoreBlockKind,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub enum CoreBlockKind {
    Amp(AmpBlock),
    Cab(CabBlock),
    IrLoader(IrLoaderBlock),

    Drive(DriveBlock),
    Compressor(CompressorBlock),
    Gate(GateBlock),
    Eq(EqBlock),
    Filter(FilterBlock),
    Wah(WahBlock),
    Pitch(PitchBlock),

    Chorus(ChorusBlock),
    Flanger(FlangerBlock),
    Phaser(PhaserBlock),
    Tremolo(TremoloBlock),
    Rotary(RotaryBlock),

    Delay(DelayBlock),
    Reverb(ReverbBlock),

    Mixer(MixerBlock),
    Split(SplitBlock),
    Merge(MergeBlock),
    Send(SendBlock),
    Return(ReturnBlock),
    VolumePan(VolumePanBlock),

    Looper(LooperBlock),
    Tuner(TunerBlock),
    Synth(SynthBlock),
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct SelectBlock {
    pub selected_block_id: BlockId,
    pub options: Vec<AudioBlock>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct AmpBlock {
    pub amp_id: String,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct CabBlock {
    pub cab_id: String,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct IrLoaderBlock {
    pub file_path: String,
    pub mix: f32,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct DriveBlock {
    pub drive: f32,
    pub tone: f32,
    pub level: f32,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct CompressorBlock {
    pub threshold: f32,
    pub ratio: f32,
    pub attack_ms: f32,
    pub release_ms: f32,
    pub makeup_gain_db: f32,
    pub mix: f32,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct GateBlock {
    pub threshold: f32,
    pub attack_ms: f32,
    pub release_ms: f32,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct EqBlock {
    pub low_gain_db: f32,
    pub mid_gain_db: f32,
    pub high_gain_db: f32,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct FilterBlock {
    pub cutoff_hz: f32,
    pub resonance: f32,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct WahBlock {
    pub position: f32,
    pub q: f32,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct PitchBlock {
    pub semitones: f32,
    pub mix: f32,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct ChorusBlock {
    pub rate_hz: f32,
    pub depth: f32,
    pub mix: f32,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct FlangerBlock {
    pub rate_hz: f32,
    pub depth: f32,
    pub feedback: f32,
    pub mix: f32,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct PhaserBlock {
    pub rate_hz: f32,
    pub depth: f32,
    pub feedback: f32,
    pub mix: f32,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct TremoloBlock {
    pub rate_hz: f32,
    pub depth: f32,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct RotaryBlock {
    pub speed: f32,
    pub mix: f32,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct DelayBlock {
    pub time_ms: f32,
    pub feedback: f32,
    pub mix: f32,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct ReverbBlock {
    pub room_size: f32,
    pub damping: f32,
    pub mix: f32,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct MixerBlock {
    pub level: f32,
    pub pan: f32,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct SplitBlock {
    pub split_type: SplitType,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub enum SplitType {
    Stereo,
    Frequency,
    DualMono,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct MergeBlock {
    pub mix_a: f32,
    pub mix_b: f32,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct SendBlock {
    pub send_id: String,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct ReturnBlock {
    pub return_id: String,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct VolumePanBlock {
    pub volume: f32,
    pub pan: f32,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct LooperBlock {
    pub max_length_seconds: u32,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct TunerBlock {
    pub reference_hz: f32,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct SynthBlock {
    pub synth_id: String,
}
