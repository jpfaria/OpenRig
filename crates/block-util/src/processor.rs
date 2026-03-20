#[derive(Debug, Clone, Default)]
pub struct TunerReading {
    pub frequency: Option<f32>,
    pub note: Option<String>,
    pub cents_off: Option<f32>,
    pub in_tune: bool,
}

pub trait TunerProcessor: Send {
    fn process(&mut self, samples: &[f32]);
    fn latest_reading(&self) -> &TunerReading;
}
