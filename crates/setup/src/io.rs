use domain::ids::{InputId, OutputId};
use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Input {
    pub id: InputId,
    pub device: usize,
    pub channels: Vec<usize>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Output {
    pub id: OutputId,
    pub device: usize,
    pub channels: Vec<usize>,
}
