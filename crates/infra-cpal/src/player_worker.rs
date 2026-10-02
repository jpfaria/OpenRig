//! Responsibility: keeps the backing-track player's ring filled from a normal-priority thread.
//!
//! Decoding, resampling and the time stretcher all run here, never on the
//! audio thread and never in the realtime scheduling class: the player must not
//! take CPU time from a chain's callback. The worker renders a short queue
//! ahead of the listener, so a speed, pitch or seek change is heard within a
//! few tens of milliseconds.

use std::path::{Path, PathBuf};
use std::sync::mpsc::{self, Receiver, RecvTimeoutError, Sender};
use std::sync::Arc;
use std::time::Duration;

use engine::player::pcm::{DecodedAudio, PlayerPcm};
use engine::player::position::HeardPosition;
use engine::player::render::PlayerRenderer;
use engine::player::resample::resample_to;
use engine::player::shared::PlayerCell;
use engine::spsc::SpscRing;

/// Reads an audio file into raw decoder output. Injected by the frontend so
/// this crate never links a decoder.
pub type PlayerDecoder = fn(&Path) -> Result<DecodedAudio, String>;

/// Frames rendered per step.
const BLOCK_FRAMES: usize = 512;
/// Frames the worker keeps queued ahead of the listener (about 85 ms at
/// 48 kHz): enough to ride out a busy normal-priority thread, short enough
/// that a change is heard almost at once.
const QUEUE_FRAMES: usize = 4096;
/// How long the thread sleeps when there is nothing to render.
const IDLE: Duration = Duration::from_millis(3);

pub(crate) enum PlayerRequest {
    Load(PathBuf),
    Attach {
        ring: Arc<SpscRing<f32>>,
        sample_rate: u32,
    },
    Detach,
}

/// The worker's whole state, driven one [`PlayerWorker::step`] at a time.
pub(crate) struct PlayerWorker {
    shared: PlayerCell,
    decode: PlayerDecoder,
    track: Option<PathBuf>,
    /// The track as decoded (`original`) or already resampled for an output.
    pcm: Option<Arc<PlayerPcm>>,
    original: bool,
    output: Option<(Arc<SpscRing<f32>>, u32)>,
    renderer: Option<PlayerRenderer>,
    heard: HeardPosition,
    seek_epoch: u64,
    generation: u64,
    /// A flush the output has not confirmed yet, and where to resume after it.
    pending: Option<(u64, f64)>,
    block: Vec<f32>,
}

impl PlayerWorker {
    pub(crate) fn new(shared: PlayerCell, decode: PlayerDecoder) -> Self {
        Self {
            seek_epoch: shared.seek_request().0,
            generation: shared.generation(),
            shared,
            decode,
            track: None,
            pcm: None,
            original: false,
            output: None,
            renderer: None,
            heard: HeardPosition::default(),
            pending: None,
            block: vec![0.0; BLOCK_FRAMES * 2],
        }
    }

    pub(crate) fn handle(&mut self, request: PlayerRequest) {
        match request {
            PlayerRequest::Load(path) => self.load(path),
            PlayerRequest::Attach { ring, sample_rate } => {
                let at = self.heard_now();
                self.output = Some((ring, sample_rate));
                self.build_renderer();
                self.reposition(at);
            }
            PlayerRequest::Detach => {
                let at = self.heard_now();
                self.output = None;
                self.pending = None;
                self.reposition(at);
            }
        }
    }

    /// One pass: settle a confirmed flush, follow seeks and settings, top the
    /// queue up, notice the end of the track, publish the heard position.
    pub(crate) fn step(&mut self) {
        if let Some((epoch, at)) = self.pending {
            if !self.shared.flush_done(epoch) {
                return;
            }
            self.pending = None;
            self.restart_at(at);
        }
        let (seek_epoch, seconds) = self.shared.seek_request();
        if seek_epoch != self.seek_epoch {
            self.seek_epoch = seek_epoch;
            self.reposition(seconds);
            return;
        }
        let generation = self.shared.generation();
        if generation != self.generation {
            self.generation = generation;
            let restart = self
                .renderer
                .as_mut()
                .is_some_and(|renderer| renderer.apply(self.shared.settings()));
            if restart {
                let at = self.heard_now();
                self.reposition(at);
                return;
            }
        }
        self.fill_queue();
        self.finish_if_ended();
        let heard = self.heard_now();
        self.shared.set_position_seconds(heard);
    }

    fn load(&mut self, path: PathBuf) {
        self.shared.set_loading(true);
        self.shared.set_failed(false);
        self.renderer = None;
        self.pcm = None;
        match (self.decode)(&path).and_then(PlayerPcm::from_decoded) {
            Ok(pcm) => {
                self.shared.set_duration_seconds(pcm.duration_seconds());
                self.pcm = Some(Arc::new(pcm));
                self.original = true;
                self.track = Some(path);
                self.build_renderer();
            }
            Err(error) => {
                log::warn!("[player] cannot load '{}': {error}", path.display());
                self.track = None;
                self.shared.set_duration_seconds(0.0);
                self.shared.set_failed(true);
            }
        }
        self.shared.set_loading(false);
        self.reposition(0.0);
    }

    /// A renderer for the attached output's rate, resampling the track when
    /// the rates differ.
    fn build_renderer(&mut self) {
        let Some(rate) = self.output.as_ref().map(|(_, rate)| *rate) else {
            return;
        };
        if self
            .pcm
            .as_ref()
            .is_some_and(|pcm| pcm.sample_rate() != rate)
        {
            self.renderer = None;
            self.pcm = self.pcm_at(rate).map(Arc::new);
            self.original = false;
        }
        let Some(pcm) = self.pcm.as_ref() else {
            self.renderer = None;
            return;
        };
        self.generation = self.shared.generation();
        self.renderer = Some(PlayerRenderer::new(Arc::clone(pcm), self.shared.settings()));
    }

    /// The track at `rate`: resampled from the decoded original when it is
    /// still held, otherwise decoded again so quality never compounds.
    fn pcm_at(&mut self, rate: u32) -> Option<PlayerPcm> {
        let source = match self.pcm.take() {
            Some(pcm) if self.original => {
                Arc::try_unwrap(pcm).unwrap_or_else(|shared| (*shared).clone())
            }
            _ => {
                let track = self.track.as_ref()?;
                let decoded = (self.decode)(track).and_then(PlayerPcm::from_decoded);
                decoded
                    .map_err(|error| log::warn!("[player] cannot reload: {error}"))
                    .ok()?
            }
        };
        resample_to(source, rate)
            .map_err(|error| log::warn!("[player] {error}"))
            .ok()
    }

    /// Moves playback to `seconds`. With an output attached the queued audio
    /// is flushed first (the callback fades it out); without one the jump is
    /// immediate.
    fn reposition(&mut self, seconds: f64) {
        self.shared.set_position_seconds(seconds);
        if self.output.is_some() {
            let epoch = self.shared.request_flush();
            self.pending = Some((epoch, seconds));
        } else {
            self.restart_at(seconds);
        }
    }

    fn restart_at(&mut self, seconds: f64) {
        if let Some(renderer) = self.renderer.as_mut() {
            renderer.seek_seconds(seconds);
        }
        self.heard.reset(seconds, self.shared.consumed());
    }

    fn heard_now(&mut self) -> f64 {
        match self.pending {
            Some((_, at)) => at,
            None => self.heard.heard(self.shared.consumed()),
        }
    }

    fn fill_queue(&mut self) {
        let (Some((ring, _)), Some(renderer)) = (self.output.as_ref(), self.renderer.as_mut())
        else {
            return;
        };
        let limit = (QUEUE_FRAMES * 2).min(ring.capacity());
        while ring.len() + BLOCK_FRAMES * 2 <= limit && !renderer.ended() {
            let frames = renderer.render(&mut self.block);
            for &sample in &self.block[..frames * 2] {
                ring.push(sample);
            }
            self.heard.pushed(frames, renderer.position_seconds());
            if frames < BLOCK_FRAMES {
                break;
            }
        }
    }

    /// The track played out: stop and rewind, as a tape deck would.
    fn finish_if_ended(&mut self) {
        let drained = self.output.as_ref().is_some_and(|(ring, _)| ring.len() < 2);
        let ended = self.renderer.as_ref().is_some_and(|r| r.ended());
        if ended && drained && self.shared.is_playing() {
            self.shared.set_playing(false);
            self.restart_at(0.0);
        }
    }
}

/// The running worker thread. Dropping it asks the thread to exit without
/// waiting for it, so a decode in progress never blocks the caller.
pub(crate) struct PlayerWorkerHandle {
    requests: Sender<PlayerRequest>,
}

impl PlayerWorkerHandle {
    pub(crate) fn send(&self, request: PlayerRequest) {
        if self.requests.send(request).is_err() {
            log::warn!("[player] worker thread is gone");
        }
    }
}

pub(crate) fn spawn_player_worker(
    shared: PlayerCell,
    decode: PlayerDecoder,
) -> std::io::Result<PlayerWorkerHandle> {
    let (requests, inbox) = mpsc::channel();
    // Built on the caller's thread so it records the seek epoch as of now: a
    // seek requested right after the spawn is then never mistaken for old.
    let worker = PlayerWorker::new(shared, decode);
    std::thread::Builder::new()
        .name("player-stream".into())
        .spawn(move || run(worker, inbox))?;
    Ok(PlayerWorkerHandle { requests })
}

fn run(mut worker: PlayerWorker, inbox: Receiver<PlayerRequest>) {
    loop {
        match inbox.recv_timeout(IDLE) {
            Ok(request) => worker.handle(request),
            Err(RecvTimeoutError::Timeout) => {}
            Err(RecvTimeoutError::Disconnected) => return,
        }
        worker.step();
    }
}

#[cfg(test)]
#[path = "player_worker_tests.rs"]
mod tests;
