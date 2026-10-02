//! Responsibility: manages the fixed pool of sounding drum voices.

use super::kit::DrumKit;
use super::voice::Voice;

pub const MAX_VOICES: usize = 64;

pub(crate) struct VoicePool {
    voices: [Voice; MAX_VOICES],
    next_seq: u64,
}

impl VoicePool {
    pub fn new() -> Self {
        Self {
            voices: [Voice::SILENT; MAX_VOICES],
            next_seq: 0,
        }
    }

    /// Starts `voice` in a free slot, stealing the oldest one when full.
    pub fn start(&mut self, mut voice: Voice) {
        voice.active = true;
        voice.seq = self.next_seq;
        self.next_seq = self.next_seq.wrapping_add(1);
        let slot = match self.voices.iter().position(|v| !v.active) {
            Some(free) => free,
            None => self
                .voices
                .iter()
                .enumerate()
                .min_by_key(|(_, v)| v.seq)
                .map(|(i, _)| i)
                .unwrap_or(0),
        };
        self.voices[slot] = voice;
    }

    /// Fades out every sounding voice of `group`.
    pub fn choke(&mut self, group: u8, fade_len: u32) {
        for voice in self.voices.iter_mut() {
            if voice.active && voice.choke_group == Some(group) {
                voice.choke(fade_len);
            }
        }
    }

    pub fn clear(&mut self) {
        for voice in self.voices.iter_mut() {
            voice.active = false;
        }
    }

    /// Adds every sounding voice of `kit` into the buffers.
    pub fn render(&mut self, kit: &DrumKit, left: &mut [f32], right: &mut [f32]) {
        for voice in self.voices.iter_mut().filter(|v| v.active) {
            let samples = kit
                .piece(voice.role)
                .and_then(|p| p.layers.get(voice.layer))
                .and_then(|l| l.samples.get(voice.sample));
            match samples {
                Some(samples) => voice.render(samples, left, right),
                None => voice.active = false,
            }
        }
    }
}
