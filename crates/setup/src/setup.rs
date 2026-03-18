use domain::ids::SetupId;
use serde::{Deserialize, Serialize};

use crate::device::{InputDevice, OutputDevice};
use crate::io::{Input, Output};
use crate::track::Track;

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Setup {
    pub id: SetupId,
    pub name: String,
    pub input_devices: Vec<InputDevice>,
    pub output_devices: Vec<OutputDevice>,
    pub inputs: Vec<Input>,
    pub outputs: Vec<Output>,
    pub tracks: Vec<Track>,
}
