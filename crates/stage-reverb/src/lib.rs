//! Reverb stage implementations.
pub mod plate;
use stage_core::NamedModel;
pub enum ReverbModel {
    Plate,
    Spring,
    Hall,
    Room,
}
impl NamedModel for ReverbModel {
    fn model_key(&self) -> &'static str {
        match self {
            ReverbModel::Plate => "plate",
            ReverbModel::Spring => "spring",
            ReverbModel::Hall => "hall",
            ReverbModel::Room => "room",
        }
    }
    fn display_name(&self) -> &'static str {
        match self {
            ReverbModel::Plate => "Plate Reverb",
            ReverbModel::Spring => "Spring Reverb",
            ReverbModel::Hall => "Hall Reverb",
            ReverbModel::Room => "Room Reverb",
        }
    }
}
pub struct ReverbParams {
    pub room_size: f32,
    pub damping: f32,
    pub mix: f32,
}
impl Default for ReverbParams {
    fn default() -> Self {
        Self {
            room_size: 0.45,
            damping: 0.35,
            mix: 0.25,
        }
    }
}
