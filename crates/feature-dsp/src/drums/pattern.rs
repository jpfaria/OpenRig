//! Responsibility: describes a looping run of beat-positioned drum hits.

use super::role::DrumRole;

#[derive(Clone, Copy, Debug, PartialEq)]
pub struct DrumHit {
    /// Position in beats from the start of the pattern.
    pub beat: f64,
    pub role: DrumRole,
    /// 0.0 ..= 1.0.
    pub velocity: f32,
}

#[derive(Clone, Debug, PartialEq)]
pub struct DrumPattern {
    beats: f64,
    hits: Vec<DrumHit>,
}

impl DrumPattern {
    /// Wraps every hit into `[0, beats)` and sorts them by position.
    pub fn new(beats: f64, mut hits: Vec<DrumHit>) -> Self {
        let beats = if beats.is_finite() && beats > 0.0 {
            beats
        } else {
            4.0
        };
        for hit in &mut hits {
            hit.beat = hit.beat.rem_euclid(beats);
            if hit.beat >= beats {
                hit.beat = 0.0;
            }
        }
        hits.sort_by(|a, b| a.beat.total_cmp(&b.beat));
        Self { beats, hits }
    }

    pub fn beats(&self) -> f64 {
        self.beats
    }

    pub fn hits(&self) -> &[DrumHit] {
        &self.hits
    }

    /// Index of the first hit whose absolute position (`origin + beat`) is
    /// past `from` (or at it, when `inclusive`).
    pub(crate) fn first_from(&self, origin: f64, from: f64, inclusive: bool) -> usize {
        self.hits.partition_point(|hit| {
            let at = origin + hit.beat;
            if inclusive {
                at < from
            } else {
                at <= from
            }
        })
    }
}
