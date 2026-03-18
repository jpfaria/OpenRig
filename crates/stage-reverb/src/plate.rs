use stage_core::MonoProcessor;
use crate::ReverbParams;
pub struct PlateReverb {
    params: ReverbParams,
    combs: [CombFilter; 4],
    allpasses: [AllpassFilter; 2],
}
impl PlateReverb {
    pub fn new(params: ReverbParams, sample_rate: f32) -> Self {
        let scale = sample_rate / 44_100.0;
        let mut combs = [
            CombFilter::new((1116.0 * scale) as usize),
            CombFilter::new((1188.0 * scale) as usize),
            CombFilter::new((1277.0 * scale) as usize),
            CombFilter::new((1356.0 * scale) as usize),
        ];
        for comb in &mut combs {
            comb.set_feedback(room_feedback(params.room_size));
            comb.set_damping(params.damping);
        }
        let allpasses = [
            AllpassFilter::new((225.0 * scale) as usize, 0.5),
            AllpassFilter::new((556.0 * scale) as usize, 0.5),
        ];
        Self {
            params,
            combs,
            allpasses,
        }
    }
    pub fn set_room_size(&mut self, room_size: f32) {
        self.params.room_size = room_size.clamp(0.0, 1.0);
        let feedback = room_feedback(self.params.room_size);
        for comb in &mut self.combs {
            comb.set_feedback(feedback);
        }
    }
    pub fn set_damping(&mut self, damping: f32) {
        self.params.damping = damping.clamp(0.0, 1.0);
        for comb in &mut self.combs {
            comb.set_damping(self.params.damping);
        }
    }
    pub fn set_mix(&mut self, mix: f32) {
        self.params.mix = mix.clamp(0.0, 1.0);
    }
}
impl MonoProcessor for PlateReverb {
    fn process_sample(&mut self, input: f32) -> f32 {
        let mut wet = 0.0;
        for comb in &mut self.combs {
            wet += comb.process(input);
        }
        wet /= self.combs.len() as f32;
        for allpass in &mut self.allpasses {
            wet = allpass.process(wet);
        }
        (1.0 - self.params.mix).mul_add(input, self.params.mix * wet)
    }
}
fn room_feedback(room_size: f32) -> f32 {
    (0.2 + room_size.clamp(0.0, 1.0) * 0.77).clamp(0.0, 0.97)
}
struct CombFilter {
    buffer: Vec<f32>,
    index: usize,
    feedback: f32,
    filter_store: f32,
    damping: f32,
}
impl CombFilter {
    fn new(size: usize) -> Self {
        Self {
            buffer: vec![0.0; size.max(1)],
            index: 0,
            feedback: 0.7,
            filter_store: 0.0,
            damping: 0.2,
        }
    }
    fn set_feedback(&mut self, feedback: f32) {
        self.feedback = feedback;
    }
    fn set_damping(&mut self, damping: f32) {
        self.damping = damping.clamp(0.0, 1.0);
    }
    fn process(&mut self, input: f32) -> f32 {
        let output = self.buffer[self.index];
        self.filter_store = (output * (1.0 - self.damping)) + (self.filter_store * self.damping);
        self.buffer[self.index] = input + (self.filter_store * self.feedback);
        self.index = (self.index + 1) % self.buffer.len();
        output
    }
}
struct AllpassFilter {
    buffer: Vec<f32>,
    index: usize,
    feedback: f32,
}
impl AllpassFilter {
    fn new(size: usize, feedback: f32) -> Self {
        Self {
            buffer: vec![0.0; size.max(1)],
            index: 0,
            feedback,
        }
    }
    fn process(&mut self, input: f32) -> f32 {
        let buffered = self.buffer[self.index];
        let output = -input + buffered;
        self.buffer[self.index] = input + (buffered * self.feedback);
        self.index = (self.index + 1) % self.buffer.len();
        output
    }
}
