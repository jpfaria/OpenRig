use serde::{Deserialize, Serialize};
use uuid::Uuid;

#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub struct TrackId(pub String);

#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub struct BlockId(pub String);

#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub struct ParameterId(pub String);

#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub struct DeviceId(pub String);

impl TrackId {
    pub fn generate() -> Self {
        Self(format!("track:{}", Uuid::new_v4()))
    }
}

impl BlockId {
    pub fn generate_for_track(track_id: &TrackId) -> Self {
        Self(format!("{}:block:{}", track_id.0, Uuid::new_v4()))
    }
}

impl ParameterId {
    pub fn for_block_path(block_id: &BlockId, path: &str) -> Self {
        Self(format!("{}::{}", block_id.0, path))
    }
}
