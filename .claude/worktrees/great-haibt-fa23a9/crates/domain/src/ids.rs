use serde::{Deserialize, Serialize};
use uuid::Uuid;

#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub struct ChainId(pub String);

#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub struct BlockId(pub String);

#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub struct ParameterId(pub String);

#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub struct DeviceId(pub String);

impl ChainId {
    pub fn generate() -> Self {
        Self(format!("chain:{}", Uuid::new_v4()))
    }
}

impl BlockId {
    pub fn generate_for_chain(chain_id: &ChainId) -> Self {
        Self(format!("{}:block:{}", chain_id.0, Uuid::new_v4()))
    }
}

impl ParameterId {
    pub fn for_block_path(block_id: &BlockId, path: &str) -> Self {
        Self(format!("{}::{}", block_id.0, path))
    }
}
