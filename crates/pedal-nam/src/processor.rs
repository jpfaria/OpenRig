pub struct NamProcessor;

impl NamProcessor {
    pub fn new(_model: &str) -> anyhow::Result<Self> {
        Ok(Self)
    }

    pub fn process_sample(&mut self, sample: f32) -> f32 {
        sample
    }
}
