//! Responsibility: renders a groove on a kit into a stereo buffer.
//!
//! Nothing here allocates, locks or blocks once built, so `render` is safe on
//! the audio thread. A kit or groove swap hands the previous one back to the
//! caller, who drops it off the audio thread.

use std::mem;
use std::sync::Arc;

use super::groove::Groove;
use super::kit::DrumKit;
use super::pattern::DrumHit;
use super::role::DrumRole;
use super::sample_select::pick_sample;
use super::sequencer::{DrumPosition, Sequencer};
use super::settings::DrumSettings;
use super::voice::{pan_gains, Voice};
use super::voice_pool::VoicePool;

/// How long a choked piece takes to fade out.
const CHOKE_FADE_SECONDS: f32 = 0.010;

pub struct DrumMachine {
    kit: Arc<DrumKit>,
    groove: Arc<Groove>,
    settings: DrumSettings,
    sequencer: Sequencer,
    voices: VoicePool,
    round_robin: [u32; DrumRole::COUNT],
    choke_fade: u32,
}

impl DrumMachine {
    pub fn new(
        sample_rate: f32,
        kit: Arc<DrumKit>,
        groove: Arc<Groove>,
        settings: DrumSettings,
    ) -> Self {
        let settings = settings.clamped();
        Self {
            kit,
            groove,
            settings,
            sequencer: Sequencer::new(sample_rate, settings.bpm),
            voices: VoicePool::new(),
            round_robin: [0; DrumRole::COUNT],
            choke_fade: (sample_rate.max(1.0) * CHOKE_FADE_SECONDS).round() as u32,
        }
    }

    pub fn play(&mut self) {
        self.sequencer.play(&self.groove);
    }

    /// Stops scheduling hits; hits already sounding ring out.
    pub fn stop(&mut self) {
        self.sequencer.stop();
    }

    pub fn is_playing(&self) -> bool {
        self.sequencer.is_playing()
    }

    pub fn trigger_fill(&mut self) {
        self.sequencer.trigger_fill(&self.groove);
    }

    pub fn settings(&self) -> DrumSettings {
        self.settings
    }

    pub fn set_settings(&mut self, settings: DrumSettings) {
        self.settings = settings.clamped();
        self.sequencer.set_bpm(self.settings.bpm);
    }

    pub fn position(&self) -> DrumPosition {
        self.sequencer.position(&self.groove)
    }

    /// Swaps the kit, silencing the old kit's voices; returns the old kit.
    pub fn replace_kit(&mut self, kit: Arc<DrumKit>) -> Arc<DrumKit> {
        self.voices.clear();
        self.round_robin = [0; DrumRole::COUNT];
        mem::replace(&mut self.kit, kit)
    }

    /// Swaps the groove at the current beat position; returns the old groove.
    pub fn replace_groove(&mut self, groove: Arc<Groove>) -> Arc<Groove> {
        let old = mem::replace(&mut self.groove, groove);
        self.sequencer.groove_changed(&self.groove);
        old
    }

    /// Overwrites `left`/`right` with the next frames of the machine.
    pub fn render(&mut self, left: &mut [f32], right: &mut [f32]) {
        let frames = left.len().min(right.len());
        let (left, right) = (&mut left[..frames], &mut right[..frames]);
        left.fill(0.0);
        right.fill(0.0);

        let Self {
            kit,
            groove,
            sequencer,
            voices,
            round_robin,
            choke_fade,
            ..
        } = self;
        let mut done = 0;
        while done < frames {
            sequencer.fire_due(groove, |hit| {
                start_hit(kit, voices, round_robin, *choke_fade, hit)
            });
            let until_hit = sequencer
                .frames_to_next_hit()
                .map_or(usize::MAX, |f| f.min(usize::MAX as u64) as usize);
            let segment = (frames - done).min(until_hit.max(1));
            let end = done + segment;
            voices.render(kit, &mut left[done..end], &mut right[done..end]);
            sequencer.advance(segment);
            done = end;
        }

        let volume = self.settings.volume;
        if volume != 1.0 {
            for s in left.iter_mut().chain(right.iter_mut()) {
                *s *= volume;
            }
        }
    }
}

fn start_hit(
    kit: &DrumKit,
    voices: &mut VoicePool,
    round_robin: &mut [u32; DrumRole::COUNT],
    choke_fade: u32,
    hit: DrumHit,
) {
    let Some(role) = kit.resolve(hit.role) else {
        return;
    };
    let Some(piece) = kit.piece(role) else {
        return;
    };
    let velocity = hit.velocity.clamp(0.0, 1.0);
    let counter = &mut round_robin[role.index()];
    let Some((layer, sample)) = pick_sample(&piece.layers, velocity, *counter) else {
        return;
    };
    *counter = counter.wrapping_add(1);
    if let Some(group) = piece.choke_group {
        voices.choke(group, choke_fade);
    }
    let gain = velocity * piece.layers[layer].gain * piece.gain;
    let (pan_l, pan_r) = pan_gains(piece.pan);
    voices.start(Voice {
        role,
        layer,
        sample,
        gain_l: gain * pan_l,
        gain_r: gain * pan_r,
        choke_group: piece.choke_group,
        ..Voice::SILENT
    });
}
