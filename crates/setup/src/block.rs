#![allow(dead_code)]

use domain::ids::BlockId;
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
    pub model: String,
    pub params: NamParams,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct NamParams {
    pub model_path: String,
    pub ir_path: Option<String>,
    pub input_db: f32,
    pub output_db: f32,
    pub noise_gate: NamNoiseGateParams,
    pub eq: NamEqParams,
    pub ir_enabled: bool,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct NamNoiseGateParams {
    pub enabled: bool,
    pub threshold_db: f32,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct NamEqParams {
    pub enabled: bool,
    pub bass: f32,
    pub middle: f32,
    pub treble: f32,
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
    pub model: String,
    pub params: AmpParams,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct AmpParams {
    pub amp_id: String,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct CabBlock {
    pub model: String,
    pub params: CabParams,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct CabParams {
    pub cab_id: String,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct IrLoaderBlock {
    pub model: String,
    pub params: IrLoaderParams,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct IrLoaderParams {
    pub file_path: String,
    pub mix: f32,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct DriveBlock {
    pub model: String,
    pub params: DriveParams,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct DriveParams {
    pub drive: f32,
    pub tone: f32,
    pub level: f32,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct CompressorBlock {
    pub model: String,
    pub params: CompressorParams,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct CompressorParams {
    pub threshold: f32,
    pub ratio: f32,
    pub attack_ms: f32,
    pub release_ms: f32,
    pub makeup_gain_db: f32,
    pub mix: f32,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct GateBlock {
    pub model: String,
    pub params: GateParams,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct GateParams {
    pub threshold: f32,
    pub attack_ms: f32,
    pub release_ms: f32,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct EqBlock {
    pub model: String,
    pub params: EqParams,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct EqParams {
    pub low_gain_db: f32,
    pub mid_gain_db: f32,
    pub high_gain_db: f32,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct FilterBlock {
    pub model: String,
    pub params: FilterParams,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct FilterParams {
    pub cutoff_hz: f32,
    pub resonance: f32,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct WahBlock {
    pub model: String,
    pub params: WahParams,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct WahParams {
    pub position: f32,
    pub q: f32,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct PitchBlock {
    pub model: String,
    pub params: PitchParams,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct PitchParams {
    pub semitones: f32,
    pub mix: f32,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct ChorusBlock {
    pub model: String,
    pub params: ChorusParams,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct ChorusParams {
    pub rate_hz: f32,
    pub depth: f32,
    pub mix: f32,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct FlangerBlock {
    pub model: String,
    pub params: FlangerParams,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct FlangerParams {
    pub rate_hz: f32,
    pub depth: f32,
    pub feedback: f32,
    pub mix: f32,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct PhaserBlock {
    pub model: String,
    pub params: PhaserParams,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct PhaserParams {
    pub rate_hz: f32,
    pub depth: f32,
    pub feedback: f32,
    pub mix: f32,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct TremoloBlock {
    pub model: String,
    pub params: TremoloParams,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct TremoloParams {
    pub rate_hz: f32,
    pub depth: f32,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct RotaryBlock {
    pub model: String,
    pub params: RotaryParams,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct RotaryParams {
    pub speed: f32,
    pub mix: f32,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct DelayBlock {
    pub model: String,
    pub params: DelayParams,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct DelayParams {
    pub time_ms: f32,
    pub feedback: f32,
    pub mix: f32,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct ReverbBlock {
    pub model: String,
    pub params: ReverbParams,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct ReverbParams {
    pub room_size: f32,
    pub damping: f32,
    pub mix: f32,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct MixerBlock {
    pub model: String,
    pub params: MixerParams,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct MixerParams {
    pub level: f32,
    pub pan: f32,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct SplitBlock {
    pub model: String,
    pub params: SplitParams,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct SplitParams {
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
    pub model: String,
    pub params: MergeParams,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct MergeParams {
    pub mix_a: f32,
    pub mix_b: f32,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct SendBlock {
    pub model: String,
    pub params: SendParams,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct SendParams {
    pub send_id: String,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct ReturnBlock {
    pub model: String,
    pub params: ReturnParams,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct ReturnParams {
    pub return_id: String,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct VolumePanBlock {
    pub model: String,
    pub params: VolumePanParams,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct VolumePanParams {
    pub volume: f32,
    pub pan: f32,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct LooperBlock {
    pub model: String,
    pub params: LooperParams,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct LooperParams {
    pub max_length_seconds: u32,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct TunerBlock {
    pub model: String,
    pub params: TunerParams,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct TunerParams {
    pub reference_hz: f32,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct SynthBlock {
    pub model: String,
    pub params: SynthParams,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct SynthParams {
    pub synth_id: String,
}
