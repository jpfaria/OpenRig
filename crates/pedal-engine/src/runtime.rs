#[derive(Debug, Default)]
pub struct TrackRuntime {
    pub queued_frames: usize,
}

#[derive(Debug, Default)]
pub struct RuntimeGraph {
    pub tracks: Vec<TrackRuntime>,
}
