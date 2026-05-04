use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Bundle {
    pub bundle_dir: String,
    pub plugins: Vec<Plugin>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Plugin {
    pub uri: String,
    pub bundle_dir: String,
    pub binary: Option<String>,
    pub doap_name: Option<String>,
    pub mod_brand: Option<String>,
    pub mod_label: Option<String>,
    pub plugin_classes: Vec<String>,
    pub ports: Vec<Port>,
}

#[allow(dead_code)]
impl Plugin {
    pub fn audio_in_count(&self) -> usize {
        self.ports
            .iter()
            .filter(|p| p.kind == PortKind::Audio && p.direction == PortDirection::Input)
            .count()
    }

    pub fn audio_out_count(&self) -> usize {
        self.ports
            .iter()
            .filter(|p| p.kind == PortKind::Audio && p.direction == PortDirection::Output)
            .count()
    }

    pub fn atom_in_count(&self) -> usize {
        self.ports
            .iter()
            .filter(|p| p.kind == PortKind::Atom && p.direction == PortDirection::Input)
            .count()
    }

    pub fn atom_out_count(&self) -> usize {
        self.ports
            .iter()
            .filter(|p| p.kind == PortKind::Atom && p.direction == PortDirection::Output)
            .count()
    }

    pub fn control_input_ports(&self) -> impl Iterator<Item = &Port> {
        self.ports
            .iter()
            .filter(|p| p.kind == PortKind::Control && p.direction == PortDirection::Input)
    }

    pub fn cv_port_count(&self) -> usize {
        self.ports.iter().filter(|p| p.kind == PortKind::Cv).count()
    }

    pub fn requires_midi(&self) -> bool {
        self.atom_in_count() > 0
            && self
                .ports
                .iter()
                .any(|p| p.kind == PortKind::Atom && p.supports_midi)
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Port {
    pub index: usize,
    pub symbol: String,
    pub name: Option<String>,
    pub kind: PortKind,
    pub direction: PortDirection,
    pub default: Option<f32>,
    pub minimum: Option<f32>,
    pub maximum: Option<f32>,
    pub is_integer: bool,
    pub is_enumeration: bool,
    pub is_toggle: bool,
    pub is_logarithmic: bool,
    pub supports_midi: bool,
    pub unit_uri: Option<String>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum PortKind {
    Audio,
    Control,
    Cv,
    Atom,
    Event,
    Other,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum PortDirection {
    Input,
    Output,
    Bidirectional,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum BlockType {
    Reverb,
    Delay,
    Mod,
    Filter,
    Dyn,
    Pitch,
    Gain,
    Util,
    Wah,
    Preamp,
    Amp,
    Cab,
    Body,
}

impl BlockType {
    pub fn crate_name(self) -> &'static str {
        match self {
            BlockType::Reverb => "block-reverb",
            BlockType::Delay => "block-delay",
            BlockType::Mod => "block-mod",
            BlockType::Filter => "block-filter",
            BlockType::Dyn => "block-dyn",
            BlockType::Pitch => "block-pitch",
            BlockType::Gain => "block-gain",
            BlockType::Util => "block-util",
            BlockType::Wah => "block-wah",
            BlockType::Preamp => "block-preamp",
            BlockType::Amp => "block-amp",
            BlockType::Cab => "block-cab",
            BlockType::Body => "block-body",
        }
    }

    pub fn registry_type_name(self) -> &'static str {
        match self {
            BlockType::Reverb => "ReverbModelDefinition",
            BlockType::Delay => "DelayModelDefinition",
            BlockType::Mod => "ModModelDefinition",
            BlockType::Filter => "FilterModelDefinition",
            BlockType::Dyn => "DynModelDefinition",
            BlockType::Pitch => "PitchModelDefinition",
            BlockType::Gain => "GainModelDefinition",
            BlockType::Util => "UtilModelDefinition",
            BlockType::Wah => "WahModelDefinition",
            BlockType::Preamp => "PreampModelDefinition",
            BlockType::Amp => "AmpModelDefinition",
            BlockType::Cab => "CabModelDefinition",
            BlockType::Body => "BodyModelDefinition",
        }
    }

    pub fn backend_kind_path(self) -> &'static str {
        match self {
            BlockType::Reverb => "ReverbBackendKind::Lv2",
            BlockType::Delay => "DelayBackendKind::Lv2",
            BlockType::Mod => "ModBackendKind::Lv2",
            BlockType::Filter => "FilterBackendKind::Lv2",
            BlockType::Dyn => "DynBackendKind::Lv2",
            BlockType::Pitch => "PitchBackendKind::Lv2",
            BlockType::Gain => "GainBackendKind::Lv2",
            BlockType::Util => "UtilBackendKind::Lv2",
            BlockType::Wah => "WahBackendKind::Lv2",
            BlockType::Preamp => "PreampBackendKind::Lv2",
            BlockType::Amp => "AmpBackendKind::Lv2",
            BlockType::Cab => "CabBackendKind::Lv2",
            BlockType::Body => "BodyBackendKind::Lv2",
        }
    }

    pub fn effect_type_const(self) -> &'static str {
        match self {
            BlockType::Reverb => "block_core::EFFECT_TYPE_REVERB",
            BlockType::Delay => "block_core::EFFECT_TYPE_DELAY",
            BlockType::Mod => "block_core::EFFECT_TYPE_MODULATION",
            BlockType::Filter => "block_core::EFFECT_TYPE_FILTER",
            BlockType::Dyn => "block_core::EFFECT_TYPE_DYNAMICS",
            BlockType::Pitch => "block_core::EFFECT_TYPE_PITCH",
            BlockType::Gain => "block_core::EFFECT_TYPE_GAIN",
            BlockType::Util => "block_core::EFFECT_TYPE_UTILITY",
            BlockType::Wah => "block_core::EFFECT_TYPE_WAH",
            BlockType::Preamp => "block_core::EFFECT_TYPE_PREAMP",
            BlockType::Amp => "block_core::EFFECT_TYPE_AMP",
            BlockType::Cab => "block_core::EFFECT_TYPE_CAB",
            BlockType::Body => "block_core::EFFECT_TYPE_BODY",
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum AudioMode {
    DualMono,
    MonoToStereo,
    TrueStereo,
    MonoOnly,
}

impl AudioMode {
    pub fn variant_path(self) -> &'static str {
        match self {
            AudioMode::DualMono => "ModelAudioMode::DualMono",
            AudioMode::MonoToStereo => "ModelAudioMode::MonoToStereo",
            AudioMode::TrueStereo => "ModelAudioMode::TrueStereo",
            AudioMode::MonoOnly => "ModelAudioMode::MonoOnly",
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum Availability {
    LinuxOnly,
    Cross,
    Skip,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Classification {
    pub block_type: Option<BlockType>,
    pub audio_mode: Option<AudioMode>,
    pub availability: Availability,
    pub skip_reason: Option<String>,
    pub brand: String,
    pub model_id: String,
    pub display_name: String,
}
