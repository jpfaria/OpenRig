//! Delay block implementations.
pub mod digital;
use stage_core::NamedModel;
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum DelayModel {
    NativeDigital,
    RustStyleDigital,
    Tape,
    Analog,
}
impl NamedModel for DelayModel {
    fn model_key(&self) -> &'static str {
        match self {
            DelayModel::NativeDigital => "native_digital",
            DelayModel::RustStyleDigital => "rust_style_digital",
            DelayModel::Tape => "tape",
            DelayModel::Analog => "analog",
        }
    }
    fn display_name(&self) -> &'static str {
        match self {
            DelayModel::NativeDigital => "Native Digital Delay",
            DelayModel::RustStyleDigital => "Rust Style Digital Delay",
            DelayModel::Tape => "Tape Delay",
            DelayModel::Analog => "Analog Delay",
        }
    }
}
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct DelayParams {
    pub time_ms: f32,
    pub feedback: f32,
    pub mix: f32,
}
impl Default for DelayParams {
    fn default() -> Self {
        Self {
            time_ms: 380.0,
            feedback: 0.35,
            mix: 0.3,
        }
    }
}
