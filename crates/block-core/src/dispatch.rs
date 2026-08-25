//! Responsibility: carries a built processor in whichever channel layout it has.

use crate::processor::{MonoProcessor, StereoProcessor};

pub enum BlockProcessor {
    Mono(Box<dyn MonoProcessor>),
    Stereo(Box<dyn StereoProcessor>),
}
